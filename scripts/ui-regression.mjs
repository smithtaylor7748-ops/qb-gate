import { chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import assert from "node:assert/strict";
// 五个入口加几条详情页。`""` 是根路径（官方账户，同时是仪表盘）。
// 前五条必须跟 src/lib/routes.ts 的 NAV 对得上，否则 CI 直接红。
// （原来还有一条 `extensions/sillytavern`：酒馆那一整块面板 —— 路径、依赖、资产、备份。
// 2026-09-29 酒馆变成内置功能，那一块搬到了软件页的 SillyTavern 卡，`software` 这一条就盖住了它。
// 同一天第三轮对调：用酒馆的那几页在账户页的「酒馆」弹窗里（弹窗没有地址，由下面「tavern panel」
// 那几条流程查）；三家的桥接设置在软件页三张 CLI 卡的最下面，默认收起，由「software page bridge sections」查。）
const ROUTES = [
  "",
  "relays",
  "subscription",
  "software",
  "extensions",
  // 0.26.0：反重力汉化与审批的插件面板（规则表里一整行正则、日志行都是最容易横向撑爆的）。
  "extensions/antigravity-ui",
  // （2026-09-25 加过 `extensions/claude-desktop-zh-cn`；09-27 Claude 汉化撤掉，那一页没了。）
  // 0.32.0：用量明细。三个 side 各走一遍 —— 它们的空态与数据源都不一样，
  // 而「Codex 没有按模型分项」那一格只有 gpt 这一档才出现。
  "usage?side=claude",
  "usage?side=gpt",
  "usage?side=antigravity",
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

/**
 * 页面报错一律算失败。除了抛出来的异常，还收 React 的「两条 key 一样」——
 * 它在开发模式下只是一句 console.error，`pageerror` 看不见。可它不只是警告：
 * key 撞了 React 就对不上号，过滤之后列表里会留着该消失的旧项
 * （2026-09-23 快速跳转实测：搜「demo-alt」列出了五个别的槽位），而这一圈一直全绿。
 */
function watchErrors(target, errors) {
  target.on("pageerror", (e) => errors.push(e.message));
  target.on("console", (m) => {
    if (m.type() === "error" && /same key/.test(m.text()))
      errors.push(m.text().slice(0, 200));
  });
}

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
  watchErrors(page, errors);
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
      watchErrors(view, errors);
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
  // 官方账户菜单栏（2026-09-23），三件事都是实测出来的 bug：
  //   ① 不在账户页时侧栏三个子项都不算「按下」—— 读屏跟高亮说同一件事；
  //   ② GPT / 反重力的账户也搜得到，点进去落在它那一边（原来只搜得到 Claude 的，
  //      而且在 GPT 页点一个 Claude 槽位，落地的还是 GPT 页）；
  //   ③ 收窄过滤之后只剩匹配的那一条 —— 原来每个 Claude 槽位都拿 `/` 当 key，
  //      先宽后窄，旧项留在列表里。
  const switcher = page.locator(".qb-account-switch");
  assert.equal(
    await switcher.locator('[aria-pressed="true"]').count(),
    0,
    "off the account pages no platform may read as pressed",
  );
  const searchBox = page.getByRole("textbox", {
    name: "搜索功能、账户、服务商或扩展",
  });
  const hits = page.locator(".qb-search-results button");
  await page.keyboard.press("Control+k");
  await searchBox.fill("account3@");
  await hits.filter({ hasText: "官方账户 · GPT" }).click();
  await switcher
    .getByRole("button", { name: "GPT", exact: true, pressed: true })
    .waitFor();
  assert.match(page.url(), /#\/$/);
  await page.keyboard.press("Control+k");
  // 逐字打，别用 `fill`：一次整串填进去只重渲一回，key 撞了也未必留下旧项 ——
  // 变异核对时（key 改回 `path`）`fill` 这一条照样绿；逐字打才复现出来，
  // 而且复现出来的是同一个账户画了三遍。
  await searchBox.fill("");
  await searchBox.pressSequentially("demo-alt");
  await hits.filter({ hasText: "demo-alt@example.com" }).first().waitFor();
  assert.equal(await hits.count(), 1, "narrowing must drop the stale rows");
  await hits.first().click();
  await switcher
    .getByRole("button", { name: "Claude", exact: true, pressed: true })
    .waitFor();
  // 草稿跨页面留得住、Esc 关得掉小窗。这两条原来测在「服务商与环境」旧页的「添加服务商」
  // 表单与「添加凭证」小窗上；那一页 2026-09-27 删了，同一条规矩改在扩展中心「手动导入」上测；
  // 2026-10-26「手动导入」撤了（阶段 5，Skills 有了自己的分页），改在 Skills 分页的「添加 Skill」上测 ——
  // 它的来源地址同样存在会话态里（`useSession`），关掉、切走、回来都还在。
  await page.goto(origin + "/#/extensions");
  await page.getByRole("tab", { name: "Skills" }).click();
  await page.getByRole("button", { name: "添加 Skill" }).click();
  const importSource = page
    .locator("dialog[open]")
    .getByLabel("GitHub 仓库 / 子目录地址，或本地目录");
  await importSource.fill("Draft survives navigation");
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  await page.getByRole("link", { name: /官方账户/ }).click();
  await page.goBack();
  await page.getByRole("button", { name: "添加 Skill" }).click();
  assert.equal(await importSource.inputValue(), "Draft survives navigation");
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  // 分页也存在会话态里：切回「发现」，后面看扩展列表的那几步才看得到卡片。
  await page.getByRole("tab", { name: "发现" }).click();
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

  // 中转站页**没有外壳分页栏**了（0.16.0）：它就是一页。原来底下那套「供应商 / 凭证 / 环境」
  // 旧页只剩深链接进得去，2026-09-27 删了 —— 下面「已经删掉的页面」那一段钉着它回不来。
  // Relay launch/close follows the actual workspace session, not router uptime.
  await page.goto(origin + "/#/relays");
  await page.locator(".qb-st-ctl").waitFor();
  assert.equal(await page.locator(".qb-page-heading h1").count(), 0);
  // 阶段 6 · U3：站点卡表头的余额 / 套餐额度。认得出的那家（演示里的 moxi）写各窗口还剩多少，
  // 普通中转站（example）照旧「站点未提供」—— 后端对它一个请求都不发。
  await page.getByTestId("site-quota").first().waitFor();
  assert.match(
    await page.getByTestId("site-quota").first().innerText(),
    /5 小时 剩 65% · 每周 剩 18%/,
  );
  assert.equal(
    await page.getByText("余额（整站共享）站点未提供", { exact: true }).count(),
    1,
  );
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

  // 已经删掉的页面（2026-09-27）：地址打进来一律回首页，旧页面的任何一块都不许再露面。
  // 快速跳转、扩展页的「中转环境模板」、设置页的「打开新手引导」原来都在往这些地址上送人。
  for (const removed of [
    "onboarding",
    "relays/new",
    "relays/providers",
    "relays/example?environment=sample-code",
    "environment/software",
    "security",
    "accounts",
  ]) {
    await page.goto(origin + "/#/" + removed);
    await page.waitForFunction(() => location.hash === "#/");
    await page.locator(".qb-page h1, .qb-page .qb-st-ctl").first().waitFor();
    for (const gone of [
      "服务商与环境",
      "新手引导",
      "添加服务商",
      "创建独立环境",
    ]) {
      assert.equal(
        await page.getByText(gone).count(),
        0,
        `/#/${removed} 还能露出旧页面的「${gone}」`,
      );
    }
  }
  // 「中转环境模板」那两条目录项也没了：直接打它的地址只落回扩展中心的列表。
  await page.goto(origin + "/#/extensions/template-claude");
  await page.getByRole("heading", { name: "扩展中心", exact: true }).waitFor();
  assert.equal(await page.getByText("创建独立环境").count(), 0);
  // 2026-09-29：酒馆变成内置功能、从扩展中心撤掉了。旧详情页地址只落回列表，列表里也没有它。
  await page.goto(origin + "/#/extensions/sillytavern");
  await page.getByRole("heading", { name: "扩展中心", exact: true }).waitFor();
  await page.locator(".qb-extension-card").first().waitFor();
  assert.equal(
    await page
      .locator(".qb-extension-card", { hasText: "SillyTavern" })
      .count(),
    0,
    "扩展中心的列表里还有酒馆",
  );
  assert.equal(await page.getByText("接入已配置的本地安装").count(), 0);
  // 快速跳转搜「酒馆」落到软件页的 SillyTavern 卡（它不在扩展目录里了，没这一条就什么都搜不到）。
  await page.keyboard.press("Control+k");
  await page
    .getByRole("textbox", { name: "搜索功能、账户、服务商或扩展" })
    .fill("酒馆");
  await page
    .locator(".qb-search-results button")
    .filter({ hasText: "酒馆 SillyTavern" })
    .click();
  await page.locator("#sillytavern").waitFor();
  assert.match(page.url(), /#\/software\?focus=sillytavern$/);
  // 快速跳转里搜站点，落在中转站页本身，不再是 `/relays/<id>`。
  await page.keyboard.press("Control+k");
  await page
    .getByRole("textbox", { name: "搜索功能、账户、服务商或扩展" })
    .fill("备用中转");
  await page
    .locator(".qb-search-results button")
    .filter({ hasText: "中转站点" })
    .first()
    .click();
  await page.locator(".qb-st-ctl").waitFor();
  assert.match(page.url(), /#\/relays$/);

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

  // 软件页的安装流程（0.29.0）。
  //
  // ⚠ 这一页此前**一个按钮都没被按过** —— 上面那 112 轮只看布局，而
  // 「装 Codex 的进度条画在 Claude Code 卡里」「Pill 和版本行互相矛盾」
  // 「安装中还能点迁移目录」这几类只有按下去才看得见。
  {
    await page.goto(origin + "/#/software", { waitUntil: "domcontentloaded" });
    await page.locator(".qb-page h1, .qb-page .qb-st-ctl").waitFor();

    // ① 反重力卡有一键安装，点了先出确认框（后端会先关掉正在跑的那份，代价要事前说）。
    const agCard = page.locator(".card").filter({
      has: page.getByRole("heading", {
        name: "反重力（Google Antigravity）",
        exact: true,
      }),
    });
    await agCard
      .getByRole("button", { name: /^(安装|更新 \/ 重装)$/ })
      .first()
      .click();
    const dialog = page.locator("dialog[open]");
    await dialog.waitFor();
    // 代价必须写在框里，不是悬停提示。
    assert.match(
      await dialog.innerText(),
      /会先被关掉/,
      "反重力安装确认框要事前说明「正在跑的会先被关掉」",
    );
    assert.match(
      await dialog.innerText(),
      /Google/,
      "确认框要说明校验的是 Google 签名",
    );
    await assertNoOverflow(page, "软件页 反重力安装确认框");
    await page.keyboard.press("Escape");
    await page.waitForTimeout(150);
    assert.equal(await page.locator("dialog[open]").count(), 0);
    report.push({
      route: "software:antigravity-install",
      width: 1180,
      theme: "light",
      passed: true,
    });

    // ② Gemini CLI 的安装走面板内进度，不再是「弹个窗口」。
    //    演示模式下它同步返回一句结论 —— 断言的是**那句结论真的出现在界面上**，
    //    而不是「已打开安装窗口」那种不代表成功的话。
    const gemCard = page.locator(".card").filter({
      has: page.getByRole("heading", { name: "Gemini CLI", exact: true }),
    });
    await gemCard.getByRole("button", { name: /用 npm (安装|重装)/ }).click();
    await page
      .getByText(/已装好（npm 全局）/)
      .first()
      .waitFor();
    await assertNoOverflow(page, "软件页 Gemini CLI 安装");
    report.push({
      route: "software:gemini-install",
      width: 1180,
      theme: "light",
      passed: true,
    });

    // ③ Pill 与版本行不许互相矛盾。
    //    七张卡逐张查：Pill 说「未安装」时版本行也得说「未安装」，反过来也一样。
    await page.goto(origin + "/#/software", { waitUntil: "domcontentloaded" });
    await page.locator(".qb-page h1, .qb-page .qb-st-ctl").waitFor();
    const checked = await page.evaluate(() => {
      const bad = [];
      const seen = [];
      for (const card of document.querySelectorAll(".card")) {
        const title = card.querySelector("h2, h3")?.textContent?.trim() ?? "?";
        const pills = [...card.querySelectorAll(".pill")]
          .map((p) => p.textContent.trim())
          .join(" | ");
        // 「版本」那一栏的大字。
        const metric = card.querySelector(".metric-v")?.textContent?.trim();
        if (!pills || metric === undefined) continue;
        seen.push(title);
        const pillSaysMissing = /未安装|未装/.test(pills);
        const lineSaysMissing = metric === "未安装";
        if (pillSaysMissing !== lineSaysMissing) {
          bad.push(`${title}：Pill「${pills}」 vs 版本行「${metric}」`);
        }
      }
      return { bad, seen };
    });
    // ⛔ **先证明这条断言不是空的。** 选择器一改（`.pill` / `.metric-v` 换了类名、
    // 卡片结构变了），上面那个循环会一张卡都不进，`bad` 恒为空数组、测试恒绿 ——
    // 这正是这个仓库反复踩的那一类：衡量它的那个量本身是假的
    // （`real_multiplier` 恒等于两数相乘、`sockets` 数的是连接数不是注入成功数）。
    assert.ok(
      checked.seen.length >= 4,
      `Pill/版本行一致性检查只看到 ${checked.seen.length} 张卡（${checked.seen.join("、")}）—— ` +
        `选择器多半失效了，这条断言现在什么都没在测`,
    );
    assert.deepEqual(
      checked.bad,
      [],
      "同一张卡上 Pill 与版本行给出了互相矛盾的结论",
    );
    report.push({
      route: "software:pill-matches-version",
      width: 1180,
      theme: "light",
      passed: true,
    });
  }

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
    watchErrors(view, errors);
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
  await page.getByRole("button", { name: "GPT", exact: true }).click();
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
  // 0.27.0：三个动作都是磁贴，按 testid 找 —— 磁贴的可访问名是「名字 + 副标题」，
  // 而副标题会随运行状态变，用名字精确匹配必然时红时绿。
  const closeCodex = page.getByTestId("gpt-tile-close");
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
  // 2026-09-23：登录正常时不再画「已登录 · 本地凭据」，行上也没有「打开」了
  // （起桌面端走右边的磁贴；没登录的槽位才留「登录」）。
  const firstCodex = page.getByTestId("codex-slot").first();
  assert.doesNotMatch(await firstCodex.innerText(), /本地凭据/);
  assert.equal(
    await firstCodex.getByRole("button", { name: "打开", exact: true }).count(),
    0,
    "已登录的槽位上不该再有「打开」",
  );
  // ⛔ 联网额度只手动刷新：打开页面只有本机快照；点那一行的刷新图标，
  // 只有那一行变成「在线」。
  assert.match(await firstCodex.innerText(), /本地快照/);
  await firstCodex
    .getByRole("button", { name: "联网刷新这个槽位的额度", exact: true })
    .click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[data-testid="codex-slot"]')
        ?.textContent?.includes("在线 ·") === true,
  );
  assert.doesNotMatch(
    await page.getByTestId("codex-slot").nth(1).innerText(),
    /在线 ·/,
    "只该问点的那一行",
  );
  // 切换永远要确认（换的是激活账户，那是个决定），只是措辞按有没有在跑改：
  // 刚才已经把桌面端关掉了，所以这里该说「直接切」。
  await page
    .getByTestId("codex-slot")
    .nth(1)
    .getByRole("button", { name: "切换", exact: true })
    .click();
  await page.getByRole("dialog").waitFor();
  assert.match(await page.getByRole("dialog").innerText(), /直接切/);
  await page.getByRole("button", { name: "取消", exact: true }).click();
  assert.match(
    await page.getByTestId("codex-slot").first().innerText(),
    /当前槽位/,
  );
  // 0.27.0：桌面端没在跑的时候，「启动」不该再弹那个「关闭旧窗口并打开」——
  // 使用者的原话是「每次点启动都会跳这个」。起完再点一次才该弹。
  await page.getByTestId("gpt-tile-launch").click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[data-testid="gpt-tile-launch"]')
        ?.textContent?.includes("运行中") === true,
  );
  assert.equal(
    await page.getByRole("dialog").count(),
    0,
    "桌面端没在跑时不该弹启动确认框",
  );
  await page.getByTestId("gpt-tile-launch").click();
  await page
    .getByRole("button", { name: "关闭旧窗口并打开", exact: true })
    .waitFor();
  assert.match(await page.getByRole("dialog").innerText(), /正在运行的任务/);
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
    await page.getByRole("button", { name: "GPT", exact: true }).click();
    await assertFixedAccounts(page, `GPT ${width}x${height}`);
    // 0.26.0：反重力那一侧同一套固定高度断言，四档一个都不许裁。
    await page.getByRole("button", { name: "反重力", exact: true }).click();
    await page.getByTestId("antigravity-accounts").waitFor();
    await assertFixedAccounts(page, `Antigravity ${width}x${height}`);
    await page.getByRole("button", { name: "Claude", exact: true }).click();
  }
  // 反重力侧的交互：账户切换、两半的徽标、并入、启动要确认、
  // 汉化引擎（弹窗里）附加 / 停止翻真翻假、用量卡的配额格与明细弹窗。
  await page.setViewportSize({ width: 1320, height: 940 });
  // 阶段 6 · U2：Claude 那一行的刷新图标也联网了，但还是只手动、只问点的那一行。
  // 打开页面是本机读数（桌面端样本 / Claude Code 快照）；点第二行的图标，只有它换成「官方接口」。
  const claudeRows = page.locator(".account-slot-list .slotrow");
  assert.doesNotMatch(
    await page.locator(".account-slot-list").innerText(),
    /官方接口/,
    "打开页面不该联网",
  );
  await claudeRows
    .nth(1)
    .getByRole("button", { name: "联网刷新这个账户的额度", exact: true })
    .click();
  await page.waitForFunction(
    () =>
      document
        .querySelectorAll(".account-slot-list .slotrow")[1]
        ?.textContent?.includes("官方接口 ·") === true,
  );
  assert.doesNotMatch(
    await claudeRows.first().innerText(),
    /官方接口/,
    "只该问点的那一行",
  );
  await page.getByRole("button", { name: "反重力", exact: true }).click();
  // 0.32.0：两个页签合成一份列表 —— 演示三条账户，第一条两半都登了且激活。
  await page.getByTestId("ag-slot").first().waitFor();
  assert.equal(await page.getByTestId("ag-slot").count(), 3);
  assert.match(
    await page.getByTestId("ag-slot").first().innerText(),
    /someone@example\.com - 个人账户[\s\S]*当前账户/,
  );
  // 两半各自一枚徽标：第一条都登了，第二条 CLI 那一半压根儿没建。
  assert.match(
    await page.getByTestId("ag-slot").first().innerText(),
    /IDE 已登录[\s\S]*CLI 已登录/,
  );
  assert.match(
    await page.getByTestId("ag-slot").nth(1).innerText(),
    /并入…/,
    "只填了一半的行要给得出「并入…」",
  );
  // 配额格显示剩余最低的那一族（演示里 Gemini 3.7 Flash 62%），不是 0、不是总量。
  assert.match(await page.getByTestId("ag-quota").innerText(), /62%/);
  assert.match(
    await page.getByTestId("ag-quota").innerText(),
    /Gemini 3\.7 Flash/,
  );
  // 切到第二条：确认框只换激活账户，不关不起；「当前账户」跟着走，
  // 配额格随之变成「登录后可见」。
  await page
    .getByTestId("ag-slot")
    .nth(1)
    .getByRole("button", { name: "切换", exact: true })
    .click();
  assert.match(await page.getByRole("dialog").innerText(), /不关不起任何东西/);
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "切换", exact: true })
    .click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.waitForFunction(
    () =>
      document
        .querySelectorAll('[data-testid="ag-slot"]')[1]
        ?.textContent?.includes("当前账户") === true,
  );
  await page.waitForFunction(
    () =>
      document
        .querySelector('[data-testid="ag-quota"]')
        ?.textContent?.includes("登录后可见") === true,
  );
  // 切回去，后面的截图要有配额可看。
  await page
    .getByTestId("ag-slot")
    .first()
    .getByRole("button", { name: "切换", exact: true })
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "切换", exact: true })
    .click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.waitForFunction(() => !document.querySelector(".toast"));
  // ⛔ 联网额度只手动刷新（2026-09-23）：打开页面时账户行上只有 IDE 写在本机的那份；
  // 点那一行的刷新图标，才出现 Claude / Gemini × 5h / 周四格与 AI 积分，
  // 用量卡的「剩余配额」跟着换成四格里最紧的那一格（演示里是 Gemini 周 95%）。
  const agFirst = page.getByTestId("ag-slot").first();
  assert.doesNotMatch(await agFirst.innerText(), /在线 ·/);
  assert.equal(
    await agFirst.getByRole("button", { name: "打开", exact: true }).count(),
    0,
    "账户行上不该再有「打开」",
  );
  await agFirst.getByRole("button", { name: /^联网刷新 .* 的额度$/ }).click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[data-testid="ag-slot"]')
        ?.textContent?.includes("在线 ·") === true,
  );
  const agOnline = await page.getByTestId("ag-slot").first().innerText();
  assert.match(
    agOnline,
    /Claude[\s\S]*5h[\s\S]*周[\s\S]*Gemini[\s\S]*5h[\s\S]*周/,
  );
  assert.match(agOnline, /AI 积分 1,000/);
  assert.match(await page.getByTestId("ag-quota").innerText(), /95%/);
  // 多出来的那两行不许把固定高度的列顶破 —— 四档都再过一遍。
  for (const [width, height] of [
    [1320, 940],
    [1180, 820],
    [900, 740],
    [680, 640],
  ]) {
    await page.setViewportSize({ width, height });
    await assertFixedAccounts(
      page,
      `Antigravity online quota ${width}x${height}`,
    );
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  // 「并入…」两步流：点只有 IDE 那一半的第二条 → 只有 CLI 那一半的第三条
  // 出「并到这里」；并完三条变两条。**只改索引，文案里必须说清楚。**
  await page
    .getByTestId("ag-slot")
    .nth(1)
    .getByRole("button", { name: "并入…" })
    .click();
  await page
    .getByTestId("ag-slot")
    .nth(2)
    .getByRole("button", { name: "并到这里" })
    .click();
  assert.match(
    await page.getByRole("dialog").innerText(),
    /两边的目录都留在原地/,
  );
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "并成一条" })
    .click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.waitForFunction(
    () => document.querySelectorAll('[data-testid="ag-slot"]').length === 2,
  );
  await assertNoOverflow(page, "Antigravity merged slot list");
  // 0.27.0：启动确认框只在**真的有东西在跑**的时候弹。
  // 第一次点（什么都没跑）→ 不弹框，直接起；第二次点（已经在跑）→ 弹框，且报得出进程数。
  await page.getByTestId("ag-tile-hub").click();
  await page.waitForFunction(
    () =>
      document
        .querySelector('[data-testid="ag-tile-hub"]')
        ?.textContent?.includes("运行中") === true,
  );
  assert.equal(
    await page.getByRole("dialog").count(),
    0,
    "没有在跑的实例时不该弹确认框",
  );
  await page.getByTestId("ag-tile-hub").click();
  const agDialog = await page.getByRole("dialog").innerText();
  assert.match(agDialog, /单实例/);
  assert.match(agDialog, /个进程在跑/);
  await page.getByRole("button", { name: "取消", exact: true }).click();
  // 一键关闭那格把 Hub 与 IDE 一起收，收完磁贴回到「未运行」。
  await page.getByTestId("ag-tile-close").click();
  await page.getByRole("button", { name: "确认关闭", exact: true }).click();
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.waitForFunction(
    () =>
      document
        .querySelector('[data-testid="ag-tile-hub"]')
        ?.textContent?.includes("未运行") === true,
  );
  // 汉化 / 自动审批 / 高危拦截：0.30.0 起收进启动卡标题栏那颗按钮打开的弹窗。
  // 入口按钮的文字如实跟着引擎状态变（「汉化」→「汉化：开」）。
  const entry = page.getByTestId("ag-engine-entry");
  assert.equal(await entry.innerText(), "汉化");
  await entry.click();
  const engine = page.getByTestId("ag-engine-toggle");
  assert.equal(await engine.innerText(), "附加");
  await engine.click();
  await page.getByText("已附加", { exact: false }).first().waitFor();
  await page.waitForFunction(
    () =>
      document.querySelector('[data-testid="ag-engine-toggle"]')
        ?.textContent === "停止",
  );
  await assertNoOverflow(page, "Antigravity engine modal running state");
  await page.keyboard.press("Escape");
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.waitForFunction(
    () =>
      document.querySelector('[data-testid="ag-engine-entry"]')?.textContent ===
      "汉化：开",
  );
  await page.waitForFunction(() => !document.querySelector(".toast"));
  await assertNoOverflow(page, "Antigravity engine running state");
  await page.screenshot({ path: "docs/screenshots/antigravity-light.png" });
  // 用量明细（0.32.0）：不再是弹窗，而是一条**可滚动**的路由。
  // 趋势图 / 按模型分布 / 各模型配额条 / 覆盖率说明 在固定高度的账户页上摆不下。
  await page.getByRole("link", { name: "用量明细", exact: true }).click();
  await page.waitForFunction(() => location.hash.startsWith("#/usage"));
  // 用量是异步拉的。不等它到位就读文本，拿到的是还没填上数据的那一帧。
  await page
    .locator(".qb-usage")
    .getByText("消耗分布", { exact: false })
    .waitFor();
  await page.waitForFunction(
    () =>
      document
        .querySelector(".qb-usage")
        ?.textContent?.includes("gemini-3.7-flash") === true,
  );
  const usage = await page.locator(".qb-usage").innerText();
  assert.match(usage, /消耗分布/);
  assert.match(usage, /gemini-3\.7-flash/);
  assert.match(usage, /零网络/, "这一页必须写明数据来自本机");
  await assertNoOverflow(page, "usage detail page");
  // 用量明细算「官方账户」底下的一页：侧栏那一块要亮着，亮的是这一页显示的那一边；
  // 在这页换一边，「回账户页」回的就是换过去的那一边 —— 原来两边各记各的，
  // 侧栏一项都不亮，回去落在进来之前那一边。
  assert.equal(await page.locator(".qb-account-nav.active").count(), 1);
  await switcher
    .getByRole("button", { name: "反重力", exact: true, pressed: true })
    .waitFor();
  await page
    .locator(".qb-usage")
    .getByRole("button", { name: "GPT", exact: true })
    .click();
  await switcher
    .getByRole("button", { name: "GPT", exact: true, pressed: true })
    .waitFor();
  await page.getByRole("link", { name: "回账户页" }).click();
  await page.waitForFunction(() => location.hash === "#/");
  await switcher
    .getByRole("button", { name: "GPT", exact: true, pressed: true })
    .waitFor();
  // ⛔ 回来之后别用裸的按钮名字：`/usage` 自己也有一排同名的侧切换按钮，
  // 而改 hash 是**同文档导航**，React 重渲前两边都在 DOM 里 —— strict mode 当场报两个。
  await page.locator(".qb-account-switch").waitFor();
  await page
    .locator(".qb-account-switch")
    .getByRole("button", { name: "反重力", exact: true })
    .click();
  await page.getByTestId("antigravity-accounts").waitFor();
  await entry.click();
  await engine.click();
  await page.waitForFunction(
    () =>
      document.querySelector('[data-testid="ag-engine-toggle"]')
        ?.textContent === "附加",
  );
  await page.keyboard.press("Escape");
  await page.getByRole("dialog").waitFor({ state: "hidden" });
  await page.waitForFunction(() => !document.querySelector(".toast"));
  await page.getByRole("button", { name: "Claude", exact: true }).click();
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
  // 0.25.3：「发现新版本」弹窗。演示里启动那一次永远「没有新版」—— 不然上面那 151 个
  // 组合全被这个全局弹窗盖住 —— 只有设置页点「检查更新」才「查到」一个编出来的 0.99.0。
  await page.setViewportSize({ width: 1180, height: 820 });
  await page.goto(origin + "/#/settings");
  await page.getByRole("heading", { name: "设置", exact: true }).waitFor();
  await page.getByRole("button", { name: "检查更新", exact: true }).click();
  const updateDialog = page.getByRole("dialog", {
    name: "发现新版本 0.99.0",
  });
  await updateDialog.waitFor();
  await updateDialog
    .getByRole("heading", { name: "更新内容", exact: true })
    .waitFor();
  for (const width of [1180, 944, 786, 680]) {
    await page.setViewportSize({ width, height: 820 });
    await assertNoOverflow(page, `更新弹窗 ${width}px`);
  }
  await page.setViewportSize({ width: 1180, height: 820 });
  // 演示不下载、不安装、不退出：「一键更新」只会如实报错，弹窗留着、代价那句一直在。
  await updateDialog
    .getByRole("button", { name: "一键更新", exact: true })
    .click();
  await updateDialog
    .getByRole("alert")
    .filter({ hasText: "演示模式不会真的下载或安装" })
    .waitFor();
  await updateDialog.getByText("没保存的内容会丢", { exact: false }).waitFor();
  await updateDialog
    .getByRole("button", { name: "以后再说", exact: true })
    .click();
  await updateDialog.waitFor({ state: "hidden" });
  // 设置页那一行跟着说「有新版本」，还能从那里把弹窗重新叫出来。
  await page.getByText("GitHub 上有新版本 0.99.0", { exact: false }).waitFor();
  await page.getByRole("button", { name: "查看并更新", exact: true }).click();
  await updateDialog.waitFor();
  await page.keyboard.press("Escape");
  await updateDialog.waitFor({ state: "hidden" });

  // 2026-09-27：Claude 汉化撤掉了，只剩「清理汉化残留」。
  // 演示默认没有残留（README 的截图不该挂着这颗按钮）—— 先没有按钮；打开演示专用的开关、
  // 刷新之后才出现；弹窗逐样列出、代价那句（会关 Claude）常驻；过确认框清完，按钮消失。
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.goto(origin + "/#/");
  await page.getByRole("button", { name: "Claude", exact: true }).click();
  await page.getByText("Claude 桌面端", { exact: true }).first().waitFor();
  assert.equal(await page.getByTestId("claude-zh-cleanup-entry").count(), 0);
  await page.evaluate(() =>
    localStorage.setItem("qb-demo-claude-zh-leftovers", "1"),
  );
  await page.reload();
  await page.getByRole("button", { name: "Claude", exact: true }).click();
  const claudeZhEntry = page.getByTestId("claude-zh-cleanup-entry");
  await claudeZhEntry.waitFor();
  assert.equal((await claudeZhEntry.innerText()).trim(), "清理汉化残留");
  await claudeZhEntry.click();
  const claudeZhPanel = page.getByTestId("claude-zh-cleanup");
  await claudeZhPanel
    .getByText("3 个前端文件跟原版不一样", { exact: false })
    .first()
    .waitFor();
  await claudeZhPanel
    .getByText("包括它 Code 页里正在跑的会话", { exact: false })
    .first()
    .waitFor();
  for (const width of [680, 900]) {
    await page.setViewportSize({ width, height: 740 });
    await assertNoOverflow(page, `Claude 汉化残留弹窗 ${width}px`);
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  await claudeZhPanel.getByTestId("claude-zh-cleanup-run").click();
  await page
    .getByRole("button", { name: "关掉 Claude 并清理", exact: true })
    .click();
  const claudeZhOutcome = claudeZhPanel.getByTestId(
    "claude-zh-cleanup-outcome",
  );
  await claudeZhOutcome.waitFor();
  assert.match(
    await claudeZhOutcome.innerText(),
    /清完了[\s\S]*换回了原版[\s\S]*删掉了上游留下的备份/,
  );
  await page.keyboard.press("Escape");
  await claudeZhPanel.waitFor({ state: "hidden" });
  await claudeZhEntry.waitFor({ state: "hidden" });
  await page.evaluate(() =>
    localStorage.removeItem("qb-demo-claude-zh-leftovers"),
  );

  // GPT：写的是它自己的设置项，逐个列出要改的文件；GPT 自己写坏的那一份说清楚、点的时候顺手修；
  // 面板起的那份开着要先过确认框（确认框里也给出「在 GPT 自己的设置里选」那条不用关的路）；
  // 别处起的那份开着时默认那一份跳过，并说清楚原因。
  await page.getByRole("button", { name: "GPT", exact: true }).click();
  const gptZhEntry = page.getByTestId("gpt-zh-entry");
  await gptZhEntry.waitFor();
  assert.equal((await gptZhEntry.innerText()).trim(), "汉化");
  await gptZhEntry.click();
  const gptZhPanel = page.getByTestId("gpt-zh-panel");
  // 等状态读回来（逐个列出的那几份出现了）再判断要不要过确认框 ——
  // 面板起的那份开着才弹；前面的流程可能已经把它关了。
  await gptZhPanel.getByText("槽位「", { exact: false }).first().waitFor();
  await gptZhPanel
    .getByText("[desktop] 重复定义了", { exact: false })
    .first()
    .waitFor();
  await assertNoOverflow(page, "GPT 汉化弹窗");
  const gptNeedsConfirm =
    (await gptZhPanel
      .getByText("面板起的 GPT 开着", { exact: false })
      .count()) > 0;
  await gptZhPanel.getByTestId("gpt-zh-apply").click();
  if (gptNeedsConfirm)
    await page
      .getByRole("button", { name: "关掉 GPT 并修改", exact: true })
      .click();
  const gptZhOutcome = gptZhPanel.getByTestId("gpt-zh-outcome");
  await gptZhOutcome.waitFor();
  const gptZhLines = await gptZhOutcome.innerText();
  assert.match(gptZhLines, /槽位「[^」]+」：已设为中文/);
  assert.match(gptZhLines, /顺手并掉了 GPT 自己写重复的 \[desktop\]/);
  // 默认那一份要么改了、要么说清楚为什么这次没改（别处起的那份开着）。
  assert.match(gptZhLines, /默认（[\s\S]*(已设为中文|这一份这次没改)/);
  await page.keyboard.press("Escape");
  await gptZhPanel.waitFor({ state: "hidden" });
  await page.getByRole("button", { name: "Claude", exact: true }).click();

  // 2026-09-29 第三轮：账户页酒馆磁贴右上角开的是「酒馆」弹窗 —— 酒馆本身的控制面板，三页开的是同一个。
  // 使用者原话「1弹窗太小，2这个应该在软件页面，软件页面的应该在这个官方账户弹窗页面」：
  // 1320×940 下弹窗宽高都得接近满屏；分页正好那六个；桥接的字段一个都不许出现（它们搬去了软件页）；
  // 底部「X 桥接设置」落到软件页这一家那一段，而且那一段已经展开。
  const PANEL_TABS = [
    "概览",
    "控制台",
    "角色卡",
    "世界书",
    "扩展",
    "备份与回收站",
  ];
  const BRIDGE_FIELDS = [
    "附加系统提示词",
    "推理强度",
    "复制密钥",
    "记入槽位用量",
  ];
  /** 从账户页（`side` 那一边）打开「酒馆」弹窗。 */
  async function openTavernPanel(side) {
    await page.getByRole("button", { name: side, exact: true }).click();
    const corner = page.getByTestId("tavern-panel-corner");
    await corner.waitFor();
    await corner.click();
    const panel = page.getByRole("dialog", { name: "酒馆", exact: true });
    await panel.waitFor();
    return panel;
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  for (const [side, provider, bridgeTitle, bridgeField] of [
    ["Claude", "claude", "Claude 桥接设置", "附加系统提示词"],
    ["GPT", "gpt", "GPT 桥接设置", "推理强度"],
    ["反重力", "gemini", "Gemini 桥接设置", "Gemini CLI 模型额度"],
  ]) {
    await page.goto(origin + "/#/");
    const panel = await openTavernPanel(side);
    assert.equal(
      await page.getByTestId("tavern-panel-corner").getAttribute("aria-label"),
      "酒馆面板",
    );
    // 「太小」的回归：原来是 huge 那一档（1180 宽、高度跟着内容走）。
    const box = await panel.boundingBox();
    assert.ok(
      box && box.width >= 1320 - 64 && box.height >= 940 - 64,
      `「酒馆」弹窗太小：${JSON.stringify(box)}`,
    );
    assert.deepEqual(await panel.getByRole("tab").allInnerTexts(), PANEL_TABS);
    // 每一页都查：桥接的字段一个都不许出现（一家的设置只在软件页那一处）。
    for (const tab of PANEL_TABS) {
      await panel.getByRole("tab", { name: tab, exact: true }).click();
      await panel.locator("#tv-tabpanel").waitFor();
      for (const text of BRIDGE_FIELDS)
        assert.equal(
          await panel.getByText(text, { exact: false }).count(),
          0,
          `「酒馆」弹窗的「${tab}」里不该有桥接的「${text}」`,
        );
    }
    await panel.getByRole("tab", { name: "概览", exact: true }).click();
    for (const width of [680, 900, 1320]) {
      await page.setViewportSize({ width, height: 900 });
      await assertNoOverflow(page, `「酒馆」弹窗（${side}）${width}px`);
    }
    await page.setViewportSize({ width: 1320, height: 940 });
    const link = panel.getByTestId("tavern-bridge-link");
    assert.equal(await link.innerText(), bridgeTitle);
    await link.click();
    await panel.waitFor({ state: "hidden" });
    assert.match(
      page.url(),
      new RegExp(`#/software\\?focus=bridge-${provider}$`),
    );
    const section = page.getByTestId(`bridge-section-${provider}`);
    assert.equal(
      await section
        .getByTestId(`bridge-toggle-${provider}`)
        .getAttribute("aria-expanded"),
      "true",
    );
    await section.getByText(bridgeField, { exact: false }).first().waitFor();
  }
  // 底部另一颗「下载与版本」落到软件页的 SillyTavern 卡。
  await page.goto(origin + "/#/");
  const panelToCard = await openTavernPanel("Claude");
  await panelToCard
    .getByRole("button", { name: "下载与版本", exact: true })
    .click();
  await page.locator("#sillytavern").waitFor();
  assert.match(page.url(), /#\/software\?focus=sillytavern$/);

  // 2026-09-29 第三轮：三条桥的设置在软件页各自那张 CLI 卡的最下面（Claude → Claude Code、GPT → Codex CLI、
  // Gemini → Gemini CLI，使用者选的）。默认收起，**收起就是没挂**：进软件页不该去探 bridge.py、读额度。
  // 每一段只装自己那条桥的全部设置，别家的一项都不许出现；三个宽度各查一次溢出。
  await page.goto(origin + "/#/software");
  await page.getByTestId("bridge-section-gemini").waitFor();
  for (const text of ["附加系统提示词", "复制密钥", "推理强度"])
    assert.equal(
      await page.getByText(text, { exact: false }).count(),
      0,
      `没展开就挂上了桥接设置的正文（「${text}」）`,
    );
  for (const [provider, card, must, mustNot] of [
    [
      "claude",
      "Claude Code",
      [
        "桥接项目目录（bridge.py 所在）",
        "附加系统提示词",
        "贴尾提示词",
        "每条消息起一次",
        "消息监控",
      ],
      ["推理强度", "状态见插件页", "提示词和监控仍在插件页"],
    ],
    [
      "gpt",
      "Codex CLI",
      ["推理强度", "记入槽位用量", "复制密钥", "ChatGPT / Codex 剩余额度"],
      ["附加系统提示词", "Gemini CLI 模型额度"],
    ],
    [
      "gemini",
      "Gemini CLI",
      ["复制密钥", "Gemini CLI 模型额度"],
      ["推理强度", "附加系统提示词", "ChatGPT / Codex 剩余额度"],
    ],
  ]) {
    // 挂在哪张卡上：那张卡的标题。
    const section = page
      .locator(".card", {
        has: page.getByRole("heading", { name: card, exact: true }),
      })
      .getByTestId(`bridge-section-${provider}`);
    const toggle = section.getByTestId(`bridge-toggle-${provider}`);
    assert.equal(await toggle.getAttribute("aria-expanded"), "false");
    await toggle.click();
    for (const text of must)
      await section.getByText(text, { exact: false }).first().waitFor();
    for (const text of mustNot)
      assert.equal(
        await section.getByText(text, { exact: false }).count(),
        0,
        `${card} 卡的桥接设置里不该有「${text}」`,
      );
    if (provider === "claude") {
      // 逐条全文：原来只在 bridge.py 自己的监控页上（「展开查看输入/输出」）。
      await section
        .getByRole("button", { name: "展开输入 / 输出" })
        .first()
        .click();
      await section.getByTestId("claude-call-detail").waitFor();
      await section
        .getByText("林澈从一排排书架间抬起头", { exact: false })
        .waitFor();
    }
    for (const width of [680, 900, 1320]) {
      await page.setViewportSize({ width, height: 900 });
      await assertNoOverflow(page, `软件页 ${card} 卡的桥接设置 ${width}px`);
    }
    await page.setViewportSize({ width: 1320, height: 940 });
    await toggle.click();
    assert.equal(await toggle.getAttribute("aria-expanded"), "false");
  }
  // 快速跳转搜「酒馆桥接」：三条都在，点了落到那一段、已经展开。
  await page.goto(origin + "/#/");
  await page.keyboard.press("Control+k");
  await page
    .getByRole("textbox", { name: "搜索功能、账户、服务商或扩展" })
    .fill("酒馆桥接");
  const bridgeHits = page.locator(".qb-search-results button");
  assert.equal(await bridgeHits.count(), 3);
  await bridgeHits.filter({ hasText: "酒馆桥接 · GPT" }).click();
  assert.match(page.url(), /#\/software\?focus=bridge-gpt$/);
  assert.equal(
    await page.getByTestId("bridge-toggle-gpt").getAttribute("aria-expanded"),
    "true",
  );
  await page
    .getByTestId("bridge-section-gpt")
    .getByText("推理强度", { exact: false })
    .first()
    .waitFor();

  // 软件页的 SillyTavern 卡（2026-09-29）：酒馆本体的下载安装 / 更新 / 卸载都在这里。
  // 「没有 Node」那一档：安装框里露出 winget 勾选，不勾不让装；导入源预填成你自己那份；
  // 装完是「面板装的」、更新框说已是最新、卸载先备份再回到「你自己的」。
  await page.evaluate(() =>
    localStorage.setItem("qb-demo-tavern-no-node", "1"),
  );
  await page.goto(origin + "/#/software?focus=sillytavern");
  await page.reload();
  const stCard = page.locator("#sillytavern");
  await stCard.waitFor();
  await stCard.getByText("你自己的", { exact: true }).waitFor();
  await stCard.getByRole("button", { name: "下载安装", exact: true }).click();
  const installDialog = page.getByRole("dialog", {
    name: "下载安装 SillyTavern？",
  });
  await installDialog.getByTestId("tavern-node-choice").waitFor();
  await installDialog.getByText("要装的版本", { exact: true }).waitFor();
  const installConfirm = installDialog.getByTestId("tavern-install-confirm");
  assert.ok(
    await installConfirm.isDisabled(),
    "没有 Node 又没勾 winget，不该让点「下载安装」",
  );
  assert.match(
    await installDialog.getByLabel("从哪一份复制").inputValue(),
    /SillyTavern$/,
  );
  assert.match(await installDialog.innerText(), /源目录不动/);
  await installDialog
    .getByText("先用 winget 装 Node.js LTS", { exact: false })
    .click();
  assert.ok(await installConfirm.isEnabled());
  for (const width of [680, 1180]) {
    await page.setViewportSize({ width, height: 900 });
    await assertNoOverflow(page, `SillyTavern 安装框 ${width}px`);
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  await installConfirm.click();
  await installDialog.waitFor({ state: "hidden" });
  await stCard.getByText("面板装的", { exact: true }).waitFor();
  await stCard.getByRole("button", { name: "检查并更新", exact: true }).click();
  const stUpdateDialog = page.getByRole("dialog", {
    name: "更新面板装的 SillyTavern？",
  });
  await stUpdateDialog
    .getByText("已经是最新的正式版", { exact: false })
    .waitFor();
  assert.ok(
    await stUpdateDialog.getByTestId("tavern-update-confirm").isDisabled(),
  );
  assert.match(await stUpdateDialog.innerText(), /sillytavern\.previous/);
  await page.keyboard.press("Escape");
  await stUpdateDialog.waitFor({ state: "hidden" });
  // 2026-09-29「版本」：最近的正式版清单 → 降级到更旧的一版（确认框写明是降级、要 Node 的勾选照旧）→
  // 「换回」留着的那一版回到原来那版；卸载也在卡上。第三轮起卡上不分页了（别的几页搬进了账户页的「酒馆」弹窗），
  // 「版本」直接摆在卡上。
  assert.equal(
    await stCard.getByRole("tab").count(),
    0,
    "SillyTavern 卡上不该再有分页",
  );
  await stCard.getByTestId("tavern-releases-list").click();
  const releases = stCard.getByTestId("tavern-releases");
  await releases.waitFor();
  const older = releases.locator(".tv-release").filter({ hasText: "1.17.0" });
  await older.getByText("比现在旧", { exact: true }).waitFor();
  await older
    .getByRole("button", { name: "降级到这一版", exact: true })
    .click();
  const downDialog = page.getByRole("dialog", {
    name: "把面板装的 SillyTavern 换成 1.17.0？",
  });
  await downDialog.getByTestId("tavern-downgrade-note").waitFor();
  const downConfirm = downDialog.getByTestId("tavern-update-confirm");
  await downDialog.getByText("要装的版本", { exact: true }).waitFor();
  assert.ok(
    await downConfirm.isDisabled(),
    "没有 Node 又没勾 winget，不该让点「降级」",
  );
  await downDialog
    .getByText("先用 winget 装 Node.js LTS", { exact: false })
    .click();
  assert.match(await downConfirm.innerText(), /降级到 1\.17\.0/);
  await downConfirm.click();
  await downDialog.waitFor({ state: "hidden" });
  await stCard.locator(".metric-v", { hasText: "v1.17.0" }).waitFor();
  await stCard.getByTestId("tavern-swap-open").click();
  const swapDialog = page.getByRole("dialog", { name: "换回留着的 1.18.1？" });
  await swapDialog.getByTestId("tavern-swap-confirm").click();
  await swapDialog.waitFor({ state: "hidden" });
  await stCard.locator(".metric-v", { hasText: "v1.18.1" }).waitFor();
  await assertNoOverflow(page, "SillyTavern 卡 · 版本");
  await stCard.getByRole("button", { name: "卸载面板装的那份" }).click();
  const uninstallDialog = page.getByRole("dialog", {
    name: "卸载面板装的 SillyTavern？",
  });
  assert.match(await uninstallDialog.innerText(), /tavern-uninstall/);
  await uninstallDialog
    .getByRole("button", { name: "备份后卸载", exact: true })
    .click();
  await stCard.getByText("你自己的", { exact: true }).waitFor();
  await page.evaluate(() => localStorage.removeItem("qb-demo-tavern-no-node"));

  // 「控制台」分页（2026-09-29；第三轮起在账户页的「酒馆」弹窗里）：面板起的酒馆没有窗口，它的输出在这里读。
  // 起不来那一档（端口被占）要认出来、给一句能照着做的话；颜色码按主题上色，不原样印出来。
  await page.evaluate(() =>
    localStorage.setItem("qb-demo-tavern-console-error", "1"),
  );
  await page.goto(origin + "/#/");
  await page.reload();
  const consolePanel = await openTavernPanel("Claude");
  await consolePanel.getByRole("tab", { name: "控制台", exact: true }).click();
  const consoleTab = consolePanel.getByTestId("tavern-console");
  await consoleTab.waitFor();
  const hints = consoleTab.getByTestId("tavern-console-hints");
  await hints.getByText("酒馆端口被占了", { exact: true }).waitFor();
  await consoleTab
    .locator(".ansi-red", { hasText: "is already in use" })
    .first()
    .waitFor();
  assert.equal(
    await consoleTab.locator(".tv-console").getByText("\u001b[31m").count(),
    0,
    "颜色码被原样印出来了",
  );
  await consoleTab.getByRole("button", { name: "上一次", exact: true }).click();
  await consoleTab
    .getByText("SillyTavern 1.18.0", { exact: true })
    .first()
    .waitFor();
  assert.equal(await consoleTab.getByTestId("tavern-console-hints").count(), 0);
  for (const width of [680, 1180]) {
    await page.setViewportSize({ width, height: 900 });
    await assertNoOverflow(page, `「酒馆」弹窗 · 控制台 ${width}px`);
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.keyboard.press("Escape");
  await consolePanel.waitFor({ state: "hidden" });
  await page.evaluate(() =>
    localStorage.removeItem("qb-demo-tavern-console-error"),
  );

  // 「角色卡」→ 详情 →「挪进回收站」（连同聊天）→「备份与回收站」里放回去 → 回到列表。
  // 「世界书」看条目（条目那个弹窗叠在「酒馆」弹窗上面，Esc 只关它自己）；「扩展」检查更新 → 落后两个提交 → 更新。
  await page.goto(origin + "/#/");
  await page.reload();
  const panel2 = await openTavernPanel("GPT");
  await panel2.getByRole("tab", { name: "角色卡", exact: true }).click();
  const chars = panel2.getByTestId("tavern-characters");
  const lin = chars.locator(".tv-char", { hasText: "林澈" });
  await lin.waitFor();
  await chars
    .locator(".tv-char", { hasText: "broken" })
    .getByText("读不出卡数据", { exact: true })
    .waitFor();
  await lin.click();
  // exact：确认框的标题里也有「林澈」，两个同时开着时不许匹配成两个。
  const charDialog = page.getByRole("dialog", { name: "林澈", exact: true });
  await charDialog
    .getByText("林澈从一排排书架间抬起头", { exact: false })
    .waitFor();
  await charDialog
    .getByRole("button", { name: "挪进回收站", exact: true })
    .click();
  const trashConfirm = page.getByRole("dialog", {
    name: "把「林澈」挪进回收站？",
  });
  await trashConfirm
    .getByText("连同它的 3 段聊天记录一起挪", { exact: true })
    .click();
  await trashConfirm
    .getByRole("button", { name: "挪进回收站", exact: true })
    .click();
  await charDialog.waitFor({ state: "hidden" });
  await chars
    .locator(".tv-char", { hasText: "林澈" })
    .waitFor({ state: "detached" });
  await panel2.getByRole("tab", { name: "备份与回收站", exact: true }).click();
  const trashList = panel2.getByTestId("tavern-trash");
  const trashed = trashList.locator(".row", { hasText: "林澈" });
  await trashed.waitFor();
  assert.match(await trashed.innerText(), /chats/);
  await trashed.getByRole("button", { name: "放回去", exact: true }).click();
  await trashList.getByText("回收站是空的", { exact: true }).waitFor();
  await panel2.getByRole("tab", { name: "角色卡", exact: true }).click();
  await panel2
    .getByTestId("tavern-characters")
    .locator(".tv-char", { hasText: "林澈" })
    .waitFor();

  await panel2.getByRole("tab", { name: "世界书", exact: true }).click();
  const worlds = panel2.getByTestId("tavern-worlds");
  await worlds
    .locator(".row", { hasText: "图书馆" })
    .getByRole("button", { name: "看条目" })
    .click();
  const worldDialog = page.getByRole("dialog", {
    name: "图书馆",
    exact: true,
  });
  await worldDialog.getByText("北门", { exact: true }).waitFor();
  await worldDialog.getByText("常驻", { exact: true }).waitFor();
  await page.keyboard.press("Escape");
  await worldDialog.waitFor({ state: "hidden" });
  assert.ok(await panel2.isVisible(), "Esc 关条目弹窗时把「酒馆」弹窗一起关了");
  assert.match(
    await worlds.locator(".row", { hasText: "残缺" }).innerText(),
    /JSON 读不懂/,
  );

  await panel2.getByRole("tab", { name: "扩展", exact: true }).click();
  const exts = panel2.getByTestId("tavern-extensions");
  const memory = exts.locator(".tv-ext", { hasText: "记忆增强（演示）" });
  await memory.getByRole("button", { name: "检查更新", exact: true }).click();
  await memory.getByText("远端多了 2 个提交", { exact: true }).waitFor();
  await memory.getByRole("button", { name: "更新", exact: true }).click();
  await memory
    .getByRole("button", { name: "更新", exact: true })
    .waitFor({ state: "detached" });
  await exts
    .locator(".tv-ext", { hasText: "leftover-folder" })
    .getByText("没有 manifest.json", { exact: true })
    .waitFor();
  // 每一页在窄屏上都不许横向溢出。
  for (const tab of PANEL_TABS) {
    await panel2.getByRole("tab", { name: tab, exact: true }).click();
    await page.setViewportSize({ width: 680, height: 900 });
    await assertNoOverflow(page, `「酒馆」弹窗 · ${tab} 680px`);
    await page.setViewportSize({ width: 1320, height: 940 });
  }
  await panel2.getByRole("tab", { name: "概览", exact: true }).click();
  await page.keyboard.press("Escape");
  await panel2.waitFor({ state: "hidden" });

  // 中转站重构阶段 1：路由设置弹窗（故障转移、派单顺序、超时、熔断、整流器）。
  // 演示里 Claude Code 的高倍组熔断着：「立刻重测」之后要转成半开，按钮上的「N 条熔断」跟着变。
  await page.goto(origin + "/#/relays");
  await page.getByRole("button", { name: /路由设置 · 故障转移/ }).click();
  const routerDialog = page.getByRole("dialog");
  await routerDialog.getByText("派单顺序（替补）", { exact: true }).waitFor();
  for (const width of [1320, 680]) {
    await page.setViewportSize({ width, height: 940 });
    await assertNoOverflow(page, `router settings ${width}`);
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  await routerDialog
    .getByRole("button", { name: "立刻重测", exact: true })
    .click();
  await routerDialog.getByText("半开 · 下一发回探", { exact: true }).waitFor();
  assert.equal(
    await routerDialog
      .getByRole("button", { name: "立刻重测", exact: true })
      .count(),
    0,
    "重测之后那条线还显示着「立刻重测」",
  );
  await routerDialog
    .getByLabel("当前那条线在交出回复之前失败时，这一发改用下面排的下一条")
    .uncheck();
  await routerDialog.getByRole("button", { name: "保存", exact: true }).click();
  await page.getByText("路由设置已保存，下一发请求就按新的来").waitFor();
  await page.keyboard.press("Escape");
  await routerDialog.waitFor({ state: "hidden" });
  // 存进去的那一份读回来还是关着（演示后端跟真后端一样有状态）。
  await page.getByRole("button", { name: /路由设置 · 故障转移/ }).click();
  await page.getByText("派单顺序（替补）", { exact: true }).waitFor();
  assert.equal(
    await page
      .getByRole("dialog")
      .getByLabel("当前那条线在交出回复之前失败时，这一发改用下面排的下一条")
      .isChecked(),
    false,
    "关掉的故障转移读回来又开着",
  );
  // 中转站重构阶段 3：聚合模型。勾一条线、写发布的模型、保存 —— 列出来的 id 是后端（演示后端）编的，前端不自己拼。
  const aggDialog = page.getByRole("dialog");
  assert.equal(
    await aggDialog
      .getByText("现在发布的（保存后刷新）", { exact: true })
      .count(),
    0,
    "聚合没开时也列了发布的模型",
  );
  await aggDialog
    .getByLabel(
      "把勾上的几条线的模型一起列给这个软件：选哪个模型，那一发就直达那条线",
    )
    .check();
  await aggDialog
    .getByLabel(/放进聚合$/)
    .first()
    .check();
  await aggDialog
    .getByLabel(/发布的模型$/)
    .first()
    .fill("glm-5[1M], glm-4.7，glm-4.7");
  await aggDialog.getByRole("button", { name: "保存", exact: true }).click();
  await aggDialog
    .getByText("qbg-claude-example--glm-5[1M]", { exact: true })
    .waitFor();
  await aggDialog
    .getByText("qbg-claude-example--glm-4.7", { exact: true })
    .waitFor();
  assert.equal(
    await aggDialog
      .getByText("qbg-claude-example--glm-4.7", { exact: true })
      .count(),
    1,
    "重复写的模型发布了两次",
  );
  await page.setViewportSize({ width: 680, height: 940 });
  await assertNoOverflow(page, "router aggregate 680");
  await page.setViewportSize({ width: 1320, height: 940 });
  // 网络那一节（R6）：开到所有网卡上才出现口令与警告；代理框不回填存着的那个（会冲掉密码）。
  const netDialog = page.getByRole("dialog");
  assert.equal(
    await netDialog.getByText("路由口令", { exact: true }).count(),
    0,
    "只本机时也显示了路由口令",
  );
  await netDialog.getByLabel("所有网卡：WSL、局域网里别的机器也能连").check();
  await netDialog.getByText("路由口令", { exact: true }).waitFor();
  await page.setViewportSize({ width: 680, height: 940 });
  await assertNoOverflow(page, "router network open 680");
  await page.setViewportSize({ width: 1320, height: 940 });
  await netDialog
    .getByRole("button", { name: "扫描本机", exact: true })
    .click();
  await netDialog
    .getByRole("button", { name: "http://127.0.0.1:7890", exact: true })
    .click();
  await netDialog
    .getByRole("button", { name: "保存代理", exact: true })
    .click();
  // 等的是保存那句提示 —— 扫描出来的那个胶囊上也写着这个地址，按地址等会在存完之前就等到。
  await page.getByText("出站代理已保存：下一发中转请求就经它走").waitFor();
  await netDialog.locator("b", { hasText: "http://127.0.0.1:7890" }).waitFor();
  assert.equal(
    await netDialog.getByLabel("出站代理地址").inputValue(),
    "",
    "保存之后代理框里还留着地址（下次一保存会冲掉密码）",
  );
  await netDialog.getByLabel("只本机（127.0.0.1）—— 默认").check();
  await netDialog
    .getByText("路由口令", { exact: true })
    .waitFor({ state: "hidden" });
  await page.keyboard.press("Escape");

  // 中转站重构阶段 2：调度设置里的派单队列与测活。先把 Claude Code 的调度打开 ——
  // 关着时档位那一列是空的、也没有「钉住」。演示里低价组没过底线、在兜底档。
  // ⚠ 高倍组本来熔断着（故障档），但上面路由设置那一段已经把它「立刻重测」成半开了，这里不等「故障」。
  await page.goto(origin + "/#/relays");
  // 那颗按钮的名字就叫「智能调度」，开没开看 aria-pressed。
  const schedToggle = (pressed) =>
    page.getByRole("button", { name: "智能调度", exact: true, pressed });
  await schedToggle(false).click();
  await schedToggle(true).waitFor();
  await page.getByRole("button", { name: "调度设置", exact: true }).click();
  const schedDialog = page.getByRole("dialog");
  await schedDialog
    .getByRole("heading", { name: "派单队列", exact: true })
    .waitFor();
  await schedDialog.getByText("合格", { exact: true }).first().waitFor();
  await schedDialog.getByText("兜底", { exact: true }).first().waitFor();
  await schedDialog.getByText("活着", { exact: true }).first().waitFor();
  for (const width of [1320, 680]) {
    await page.setViewportSize({ width, height: 940 });
    await assertNoOverflow(page, `schedule dispatch queue ${width}`);
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  // 花钱测活默认关：「立刻测」只做免费的，今天花的还是 0。
  const paidProbe = schedDialog.getByLabel("开启花钱测活");
  assert.equal(await paidProbe.isChecked(), false, "花钱测活默认就开着");
  await schedDialog
    .getByRole("button", { name: "立刻测", exact: true })
    .click();
  await page
    .getByText(/^测了 \d+ 次/)
    .first()
    .waitFor();
  await schedDialog.getByText(/今天测活约 ¥0\.0000～¥0\.0000/).waitFor();
  // 打开之后再测：花钱的那几次照实记上，「今天测活约」跟着涨。
  await paidProbe.check();
  await schedDialog
    .getByRole("button", { name: "立刻测", exact: true })
    .click();
  await schedDialog
    .getByText(/今天测活约 ¥0\.0000～¥0\.0000/)
    .waitFor({ state: "hidden" });
  await paidProbe.uncheck();
  // 钉住一条：横幅出现、动作日志多一条；取消之后横幅消失。
  await schedDialog
    .getByRole("button", { name: "钉住", exact: true })
    .first()
    .click();
  await schedDialog.getByText(/^已钉住「/).waitFor();
  // 档位那一格和动作日志那一格都写「钉住」。
  await schedDialog
    .getByRole("cell", { name: "钉住", exact: true })
    .first()
    .waitFor();
  await schedDialog
    .getByRole("button", { name: "取消钉住", exact: true })
    .first()
    .click();
  await page.getByText("已取消钉住，交回调度", { exact: true }).waitFor();
  await schedDialog.getByText(/^已钉住「/).waitFor({ state: "hidden" });
  await page.keyboard.press("Escape");
  await schedDialog.waitFor({ state: "hidden" });
  // 调度开着时路由设置里的替补顺序只读（顺序归调度排）。
  await page.getByRole("button", { name: /路由设置 · 故障转移/ }).click();
  await page.getByText(/智能调度开着：替补顺序由调度按档位排/).waitFor();
  await page.keyboard.press("Escape");
  await schedToggle(true).click();
  await schedToggle(false).waitFor();

  // 中转站重构阶段 5：项目、导入链接、工具文件。
  // 项目：头部按钮写着当前项目（演示里 Claude Code 正用着「日常开发」），弹窗里说清楚只换中转那一侧。
  await page.goto(origin + "/#/relays");
  await page.getByRole("button", { name: /项目：日常开发/ }).click();
  const projDialog = page.locator("dialog[open]");
  await projDialog.getByText(/官方账户和槽位一概不动/).waitFor();
  await projDialog.getByLabel("新项目的名字").fill("写作");
  await projDialog.getByRole("button", { name: /按现在的样子新建/ }).click();
  await page.getByText("存成项目「写作」了").waitFor();
  for (const width of [1320, 680]) {
    await page.setViewportSize({ width, height: 940 });
    await assertNoOverflow(page, `projects dialog ${width}`);
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.keyboard.press("Escape");
  assert.equal(await page.locator("dialog[open]").count(), 0);
  // 导入链接：粘贴一条给 CC Switch 的链接 → 确认框里 Key 只有打码的 → 确认了才导入，导完停在中转站页。
  await page.getByRole("button", { name: "从链接导入", exact: true }).click();
  await page
    .getByLabel("导入链接")
    .fill(
      "ccswitch://v1/import?resource=provider&app=claude&name=x&endpoint=https%3A%2F%2Frelay.example.com&apiKey=sk-demo",
    );
  await page
    .getByRole("button", { name: "看看里面是什么", exact: true })
    .click();
  await page.getByText("导入站点 · Claude Code", { exact: true }).waitFor();
  await page.getByText("sk-d…7a1f", { exact: true }).waitFor();
  await page.getByText(/不会切过去/).waitFor();
  for (const width of [1320, 680]) {
    await page.setViewportSize({ width, height: 940 });
    await assertNoOverflow(page, `deeplink confirmation ${width}`);
  }
  await page.setViewportSize({ width: 1320, height: 940 });
  await page.getByRole("button", { name: "导入这条线路", exact: true }).click();
  await page.getByText(/导入了「示例中转 · 默认分组」/).waitFor();
  assert.equal(await page.locator("dialog[open]").count(), 0);
  assert.match(page.url(), /#\/relays$/);
  // 工具文件：扩展中心的新分页，三个工具都切得过去，窄屏不溢出；改一份存得上。
  await page.goto(origin + "/#/extensions");
  await page.getByRole("tab", { name: "工具文件" }).click();
  await page.getByText("workspace/SOUL.md", { exact: true }).waitFor();
  const toolPicker = page.getByRole("group", { name: "哪个工具" });
  for (const [tool, file] of [
    ["OpenClaw", "workspace/memory/2026-10-25.md"],
    ["Hermes", "memories/USER.md"],
    ["Pi", "prompts/review.md"],
  ]) {
    await toolPicker.getByRole("button", { name: tool, exact: true }).click();
    await page.getByText(file, { exact: true }).waitFor();
    await page.setViewportSize({ width: 680, height: 940 });
    await assertNoOverflow(page, `tool files ${tool} 680`);
    await page.setViewportSize({ width: 1320, height: 940 });
  }
  await page
    .locator(".qb-mcp-row", { hasText: "prompts/review.md" })
    .getByRole("button", { name: "编辑", exact: true })
    .click();
  await page.getByLabel("内容").fill("先说风险，再说改法，最后给补丁。");
  await page.getByRole("button", { name: "存", exact: true }).click();
  await page.getByText("存好了：prompts/review.md").waitFor();
  // 分页存在会话态里：切回「发现」，跟这条流程之前一样。
  await page.getByRole("tab", { name: "发现" }).click();

  assert.deepEqual(errors, []);
  const flows = [
    "catalog isolation",
    "global search and browser back, official-account side from search results",
    "draft navigation persistence (extension import form)",
    "keyboard modal close (extension import form)",
    "removed pages stay unreachable: old routes land home, template entries and the tavern gone from extensions, site search lands on the relay page, tavern search lands on the software card",
    "overview security modal",
    "system theme matches explicit theme",
    "desktop account pagination and safe login confirmation",
    "five-item diagnostics and repair review",
    "IPv6 default switch and per-adapter restore",
    "equal fixed account columns and wheel immobility",
    "station audit model entry and independent billing credentials",
    "antigravity side fixed columns, ide slot switch, slot tabs, launch confirmation, engine modal attach/stop, usage details and side sync",
    "software page: antigravity one-click install confirmation",
    "software page: gemini cli install reports a verified result",
    "software page: every pill agrees with its version row",
    "update dialog: manual check, notes, cost sentence, demo install refusal, reopen from settings",
    "Claude localization leftovers: hidden without leftovers, listed with the close-Claude cost, confirmed cleanup, entry disappears; GPT own setting with duplicate-table repair and skipped default home",
    "tavern panel from all three account pages: the same near-full-window dialog, exactly six tabs, no bridge field on any tab, no overflow at three widths, the footer lands on that provider's bridge section already expanded and on the SillyTavern card",
    "software page bridge sections: collapsed means unmounted, each CLI card carries only its own bridge (Claude with per-call detail), no overflow at three widths, quick jump lands expanded",
    "software page SillyTavern card: missing Node gates install behind the winget choice, import source prefilled, install makes it managed, update says latest, uninstall backs up first",
    "SillyTavern card versions without tabs: release list, downgrade names itself and still gates on Node, swap back to the kept copy",
    "tavern panel console tab: a failed start is recognized with a hint, colors are rendered not printed, the previous run is one click away",
    "tavern panel library tabs: character detail, to trash with its chats and back, world entries whose Esc closes only the entry dialog, extension check then update, every tab without overflow at 680px",
    "relay router settings: dispatch queue and breaker pills without overflow at two widths, retest turns a tripped line half-open, failover switch saved and read back off; network section shows the key only when open to all interfaces, scan fills the proxy, the proxy box is not prefilled after saving",
    "relay schedule dispatch: tiers and liveness without overflow at two widths, paid probing off by default and spends nothing, switched on it shows a cost, pin shows a banner and an action, unpin clears it, the manual standby order is read-only while scheduling",
    "stage 5: the project dialog says it only touches the relay side and creates from the current state, a pasted ccswitch link shows only a masked key and imports after confirming onto the relay page, the tool files tab switches between OpenClaw / Hermes / Pi without overflow and saves an edit",
  ];
  writeFileSync(
    "docs/ui-regression.json",
    JSON.stringify({ cases: report, flows, pageErrors: errors }, null, 2),
  );
  // 数字从数组里来，不手写 —— 手写的那个会跟列表漂开，而漂开之后它照样绿。
  console.log(
    `${report.length} responsive/theme routes and ${flows.length} interaction flows passed.`,
  );
} finally {
  if (browser) await browser.close();
  server.kill();
}
