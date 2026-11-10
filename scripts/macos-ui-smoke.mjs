import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import { existsSync, mkdirSync } from "node:fs";
import assert from "node:assert/strict";

const port = 20000 + Math.floor(Math.random() * 20000);
const origin = `http://localhost:${port}`;
const server = await createServer({
  mode: "demo",
  plugins: [
    {
      name: "macos-ui-smoke-gate-fixtures",
      enforce: "pre",
      transform(source, id) {
        if (
          !id.split("?")[0].replaceAll("\\", "/").endsWith("/src/lib/demo.ts")
        )
          return;
        const dispatch = "const make = FIXTURES[cmd];";
        assert.ok(
          source.includes(dispatch),
          "demo dispatch must be instrumented",
        );
        // Extend only this disposable demo server, keeping real API/IPC/UI wiring.
        return source.replace(
          dispatch,
          `const smoke = globalThis.__qbMacGateSmoke;
          smoke?.commands.push(cmd);
          const make = smoke?.fixtures[cmd] ?? FIXTURES[cmd];`,
        );
      },
    },
  ],
  server: { port, strictPort: true, watch: null },
});
let browser;
try {
  await server.listen();
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
    serviceWorkers: "block",
  });
  await context.route("**/*", (route) => {
    const url = new URL(route.request().url());
    return url.origin === origin || url.protocol === "data:"
      ? route.continue()
      : route.abort();
  });
  // WKWebView versions without requestIdleCallback must still render the app.
  await context.addInitScript(() => {
    Object.defineProperty(window, "requestIdleCallback", { value: undefined });
    const gate = {
      current_ip: null,
      allowlist: ["203.0.113.9"],
      ip_allowed: false,
      targets: [],
      all_locked: false,
      lease: { holders: {}, holder: null, granted: [], mode: null },
      watchdog_running: true,
      stale_copies: [],
      recent_log: ["2026-11-10 12:00:00 受管门禁已暂停，等待重新验证"],
      needs_reopen: "受管门禁已暂停，等待重新验证",
    };
    window.__qbMacGateSmoke = {
      commands: [],
      fixtures: {
        gate_status: () => structuredClone(gate),
        gate_check: () => {
          gate.current_ip = "203.0.113.9";
          gate.recent_log.push(
            "2026-11-10 12:01:00 已检测出口 203.0.113.9，门禁仍暂停",
          );
          return null;
        },
        gate_reopen: () => {
          gate.ip_allowed = true;
          gate.needs_reopen = null;
          gate.recent_log.push(
            "2026-11-10 12:02:00 验证通过，受管门禁已重新放行",
          );
          return "已验证并重新放行门禁";
        },
        gate_lock_all: () => {
          gate.ip_allowed = false;
          gate.needs_reopen = "受管门禁已暂停，等待重新验证";
          gate.recent_log.push("2026-11-10 12:03:00 受管门禁已关闭");
          return 0;
        },
      },
    };
  });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(`${origin}/?platform=macos#/software`, {
    waitUntil: "networkidle",
  });
  await page.getByRole("heading", { name: "软件与安装" }).waitFor();
  assert.ok(
    await page
      .getByText("Apple Silicon · 安装命令行工具", { exact: false })
      .isVisible(),
  );
  assert.equal(await page.getByText("始终开", { exact: true }).count(), 0);
  assert.ok(
    await page.getByText("本版不提供系统执行锁", { exact: false }).isVisible(),
  );
  await page
    .getByRole("heading", { name: "其他命令行工具", exact: true })
    .waitFor();
  await page
    .getByRole("tablist", { name: "酒馆的分页", exact: true })
    .waitFor();
  assert.equal(
    await page
      .getByRole("button", { name: "用官方脚本装…", exact: true })
      .count(),
    0,
  );
  await page.getByRole("link", { name: /macOS 官方安装说明/ }).waitFor();
  await page.goto(
    `${origin}/?platform=macos#/software?focus=sillytavern&tab=console`,
  );
  await expect(
    page.getByRole("tab", { name: "控制台", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  mkdirSync("test-results", { recursive: true });
  await page.screenshot({
    path: "test-results/macos-software.png",
    fullPage: true,
  });
  await page.goto(`${origin}/?platform=macos#/settings`, {
    waitUntil: "networkidle",
  });
  assert.equal(
    await page
      .getByRole("heading", { name: "启动时对齐", exact: true })
      .count(),
    0,
  );
  await page.getByText("登录 macOS 后", { exact: false }).waitFor();

  await page.goto(`${origin}/?platform=macos#/`, {
    waitUntil: "networkidle",
  });
  const commandCount = (command) =>
    page.evaluate(
      (name) =>
        window.__qbMacGateSmoke.commands.filter((cmd) => cmd === name).length,
      command,
    );
  await page.getByTitle("IP 锁 —— 点开就地处理", { exact: true }).click();
  const allowlist = page.getByRole("dialog", { name: /IP 白名单/ });
  await expect(allowlist).toBeVisible();
  await allowlist
    .getByRole("button", { name: "检测当前出口", exact: true })
    .click();
  await expect.poll(() => commandCount("gate_check")).toBe(1);
  await expect(
    allowlist.getByText("未通过或已暂停", { exact: true }),
  ).toBeVisible();
  await expect(allowlist.getByText("当前", { exact: true })).toBeVisible();
  assert.equal(await commandCount("gate_reopen"), 0);
  await page.screenshot({
    path: "test-results/macos-ip-allowlist.png",
    fullPage: true,
  });
  await page.keyboard.press("Escape");
  await expect(allowlist).not.toBeVisible();
  await page.getByRole("button", { name: /无系统执行锁/ }).click();
  const lock = page.getByRole("dialog", { name: /^执行锁/ });
  await expect(
    lock.getByRole("heading", { name: "受管启动与 IP 检查" }),
  ).toBeVisible();
  await expect(lock.getByText(/监督任务\s*运行中/)).toBeVisible();
  await expect(
    lock.getByRole("heading", { name: "ip-gate.log", exact: true }),
  ).toBeVisible();
  await expect(
    lock.getByRole("heading", { name: "运行日志", exact: true }),
  ).toBeVisible();
  await expect(
    lock.getByRole("button", {
      name: /应急解锁|立即全部上锁|应急移除软件执行锁/,
    }),
  ).toHaveCount(0);
  await lock.getByRole("button", { name: "检测当前出口", exact: true }).click();
  await expect.poll(() => commandCount("gate_check")).toBe(2);
  await expect(page.getByText("门禁检测已更新", { exact: true })).toBeVisible();
  await expect(
    lock.getByText("受管门禁已暂停，等待重新验证", { exact: true }).first(),
  ).toBeVisible();
  assert.equal(await commandCount("gate_reopen"), 0);
  await lock
    .getByRole("button", { name: "验证并重新放行", exact: true })
    .click();
  await expect.poll(() => commandCount("gate_reopen")).toBe(1);
  await expect(
    page.getByText("已验证并重新放行门禁", { exact: true }),
  ).toBeVisible();
  await expect(lock.getByRole("log").first()).toContainText(
    "验证通过，受管门禁已重新放行",
  );
  await expect(
    lock.locator(".notice--warn").filter({ hasText: "受管门禁已暂停" }),
  ).toHaveCount(0);
  const statusReads = await commandCount("gate_status");
  await lock.getByRole("button", { name: "刷新日志", exact: true }).click();
  await expect
    .poll(() => commandCount("gate_status"))
    .toBeGreaterThan(statusReads);
  await expect(
    lock.getByRole("button", { name: "刷新日志", exact: true }),
  ).toBeEnabled();
  await page.screenshot({
    path: "test-results/macos-ip-lock.png",
    fullPage: true,
  });

  await lock.getByRole("button", { name: "关闭受管门禁", exact: true }).click();
  const confirmation = page.getByRole("dialog", {
    name: "关闭受管门禁？",
    exact: true,
  });
  await expect(confirmation).toBeVisible();
  await expect(confirmation.getByText(/未保存的内容可能丢失/)).toBeVisible();
  assert.equal(await commandCount("gate_lock_all"), 0);
  await page.screenshot({
    path: "test-results/macos-ip-lock-confirm.png",
    fullPage: true,
  });
  await confirmation.getByRole("button", { name: "取消", exact: true }).click();
  await expect(confirmation).not.toBeVisible();
  assert.equal(await commandCount("gate_lock_all"), 0);
  await expect(
    lock.locator(".notice--warn").filter({ hasText: "受管门禁已暂停" }),
  ).toHaveCount(0);
  await lock.getByRole("button", { name: "关闭受管门禁", exact: true }).click();
  await confirmation
    .getByRole("button", { name: "确认关闭", exact: true })
    .click();
  await expect.poll(() => commandCount("gate_lock_all")).toBe(1);
  await expect(confirmation).not.toBeVisible();
  await expect(
    lock.locator(".notice--warn").filter({ hasText: "受管门禁已暂停" }),
  ).toBeVisible();
  assert.equal(await commandCount("gate_unlock_all"), 0);
  assert.equal(
    await page.evaluate(() =>
      window.__qbMacGateSmoke.commands.some((cmd) => /launch/.test(cmd)),
    ),
    false,
  );
  assert.deepEqual(errors, []);
  console.log(
    "macOS UI: software, capability limits, login item text, WebKit idle fallback, IP inspection, explicit reopen, logs and cancellable close confirmation passed (isolated demo; external requests blocked).",
  );
} finally {
  await browser?.close();
  await server.close();
}
