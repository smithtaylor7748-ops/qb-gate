import { chromium } from "@playwright/test";
import { createServer } from "vite";
import { existsSync, mkdirSync } from "node:fs";
import assert from "node:assert/strict";

const port = 20000 + Math.floor(Math.random() * 20000);
const origin = `http://localhost:${port}`;
const server = await createServer({
  mode: "demo",
  server: { port, strictPort: true, watch: { ignored: ["**/target/**"] } },
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
  });
  await context.route("**/*", (route) =>
    route.request().url().startsWith(origin) ||
    route.request().url().startsWith("data:")
      ? route.continue()
      : route.abort(),
  );
  // WKWebView versions without requestIdleCallback must still render the app.
  await context.addInitScript(() => {
    Object.defineProperty(window, "requestIdleCallback", { value: undefined });
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
  await page.getByRole("tab", { name: "控制台", exact: true }).waitFor();
  assert.equal(
    await page
      .getByRole("tab", { name: "控制台", exact: true })
      .getAttribute("aria-selected"),
    "true",
  );
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
  assert.deepEqual(errors, []);
  console.log(
    "macOS UI: software, capability limits, login item text and WebKit idle fallback passed.",
  );
} finally {
  await browser?.close();
  await server.close();
}
