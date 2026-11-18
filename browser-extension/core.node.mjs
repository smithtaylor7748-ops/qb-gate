// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 插件纯逻辑的单测（npm run test:browser-extension）。不起浏览器、不联网、不碰任何真实运行期状态。
// 真实浏览器里的行为由 scripts/browser-extension-e2e.mjs 验收。
import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  ALLOW_RULE_ID,
  BLOCK_RULE_ID,
  DEFAULT_SITES,
  MAX_TTL_MS,
  RESOURCE_TYPES,
  StatusTracker,
  acceptClaim,
  allowRules,
  armBackoff,
  blockRules,
  blockedPage,
  claimPagePort,
  describe,
  enhancementSummary,
  nextStep,
  normalizeSite,
  normalizeSites,
  offlineReason,
  proof,
  protectedSite,
  reconnectBackoff,
  returnTarget,
  sameSecret,
  sameStation,
  sessionCookie,
  sitePatterns,
  validPairing,
  validProbeUrl,
  validStatus,
} from "./core.js";
import { captureCurrentAccount } from "./capture.js";

const here = dirname(fileURLToPath(import.meta.url));
/** 固定扩展 ID：manifest 里的公钥决定，文档与面板（P3）都引用这个值。 */
export const EXTENSION_ID = "dffmajnemniojfblgfhenndeeelbjgoh";

const status = (extra = {}) => ({
  type: "status",
  protocol: 2,
  seq: 1,
  allow: false,
  code: "not_armed",
  reason: "插件已连接，保护尚未开启",
  armed: false,
  checking: false,
  ip: null,
  baseline: null,
  checked_at: 0,
  ttl_ms: 10_000,
  ...extra,
});
const allowed = (extra = {}) =>
  status({
    allow: true,
    code: "allowed",
    reason: "出口 IP 已核实，可以放行",
    armed: true,
    ip: "203.0.113.10",
    baseline: "203.0.113.10",
    checked_at: 1_700_000_000,
    ...extra,
  });

test("proofs match the bridge's documented WebCrypto vectors (protocol §8)", async () => {
  const token = "0123456789abcdef".repeat(4);
  const [a, b, c] = ["a".repeat(64), "b".repeat(64), "c".repeat(64)];
  assert.equal(
    await proof(token, "server", a, b),
    "1763e994b1beed96ee368aea557a2a0cf15065dd5b16e73ec2c12275432ffc99",
  );
  assert.equal(
    await proof(token, "client", a, b),
    "442e2ed3d3898870766d04d3da58681ab6c715b6cc6db65633aa0965e9ad7e9b",
  );
  assert.equal(
    await proof(token, "claim", "0123456789abcdef", c),
    "5eb5a5960c281708313c0b758e614ef42d55c17abfd2fb203fe6db266f6f529e",
  );
  assert.equal(
    await proof(token, "claimed", "0123456789abcdef", c),
    "4d26b086c90ffd9ae1cfaeacf6873686998cf209bca847a94db4c66b382ba83a",
  );
  assert.equal(sameSecret("abc", "abc"), true);
  assert.equal(sameSecret("abc", "abd"), false);
  assert.equal(sameSecret("abc", "abcd"), false);
  assert.equal(sameSecret(undefined, "abc"), false);
});

test("pairing records and claim pages are recognised strictly", () => {
  const good = { id: "0123456789abcdef", token: "a".repeat(64), port: 41234 };
  assert.equal(validPairing(good), true);
  for (const bad of [
    null,
    { ...good, id: "0123" },
    { ...good, token: "A".repeat(64) },
    { ...good, port: 0 },
    { ...good, port: 70000 },
  ])
    assert.equal(validPairing(bad), false);
  assert.equal(claimPagePort("http://127.0.0.1:41234/link#c=abc"), 41234);
  for (const url of [
    "http://localhost:41234/link",
    "https://127.0.0.1:41234/link",
    "http://127.0.0.1/link",
    "http://127.0.0.1:41234/link/app.js",
    "http://user@127.0.0.1:41234/link",
    "http://127.0.0.1.evil.test:41234/link",
    "not a url",
  ])
    assert.equal(claimPagePort(url), null, url);
});

test("claim replies: new pairing takes the token from the reply, relink must prove with the stored token", async () => {
  const claim = "c".repeat(64);
  const token = "0123456789abcdef".repeat(4);
  const id = "0123456789abcdef";
  const paired = {
    type: "paired",
    protocol: 2,
    pairing: id,
    token,
    port: 41234,
    proof: await proof(token, "claimed", id, claim),
  };
  assert.deepEqual(await acceptClaim(paired, claim, null), {
    id,
    token,
    port: 41234,
  });
  await assert.rejects(
    acceptClaim({ ...paired, proof: "0".repeat(64) }, claim, null),
    /证明/,
  );
  await assert.rejects(
    acceptClaim({ ...paired, protocol: 1 }, claim, null),
    /格式/,
  );
  const relinked = {
    ...paired,
    type: "relinked",
    token: undefined,
    port: 41999,
  };
  const existing = { id, token, port: 41234 };
  assert.deepEqual(await acceptClaim(relinked, claim, existing), {
    id,
    token,
    port: 41999,
  });
  await assert.rejects(acceptClaim(relinked, claim, null), /没有这条配对/);
  await assert.rejects(
    acceptClaim(relinked, claim, { ...existing, token: "f".repeat(64) }),
    /证明/,
  );
  await assert.rejects(acceptClaim(relinked, "d".repeat(64), existing), /证明/);
});

test("only the newest status counts, and allow expires on either clock", () => {
  const t = new StatusTracker();
  const wall = 1_700_000_000_000;
  assert.equal(t.accept(allowed({ seq: 5 }), 1_000, wall), true);
  assert.equal(t.allowed(1_000, wall), true);
  // 晚到的旧回包（序号小）盖不掉新的推送。
  assert.equal(t.accept(status({ seq: 4 }), 1_100, wall), false);
  assert.equal(t.allowed(1_100, wall), true);
  // 同一序号的心跳照样续期。
  assert.equal(t.accept(allowed({ seq: 5 }), 9_000, wall + 8_000), true);
  assert.equal(t.allowed(18_999, wall + 17_999), true);
  assert.equal(t.allowed(19_000, wall + 17_999), false, "单调钟到期");
  assert.equal(t.allowed(18_000, wall + 18_000), false, "墙上时钟到期（睡眠）");
  // 不放行立刻生效。
  t.accept(status({ seq: 6, armed: true, code: "ip_unknown" }), 9_100, wall);
  assert.equal(t.allowed(9_100, wall), false);
  // ttl 只信 60 秒；奇怪的形状一律不收。
  t.reset();
  t.accept(allowed({ seq: 1, ttl_ms: 10 ** 9 }), 0, wall);
  assert.equal(t.allowed(MAX_TTL_MS - 1, wall), true);
  assert.equal(t.allowed(MAX_TTL_MS, wall), false);
  assert.equal(t.accept({ ...allowed(), allow: "yes" }, 0, wall), false);
  assert.equal(t.accept({ ...allowed(), code: "maybe" }, 0, wall), false);
  assert.equal(validStatus({ ...allowed(), type: "offline" }), false);
});

test("a network change distrusts allows that were checked before it", () => {
  const t = new StatusTracker();
  const wall = 1_700_000_100_000;
  t.accept(allowed({ seq: 3, checked_at: 1_700_000_090 }), 0, wall);
  assert.equal(t.allowed(1, wall), true);
  t.requireFresh(wall);
  assert.equal(t.allowed(1, wall), false);
  t.accept(allowed({ seq: 3, checked_at: 1_700_000_090 }), 2, wall);
  assert.equal(t.allowed(3, wall), false, "旧的核实结果心跳不算");
  t.accept(allowed({ seq: 4, checked_at: 1_700_000_100 }), 4, wall);
  assert.equal(t.allowed(5, wall), true);
  t.reset();
  assert.equal(t.freshAfter, 0);
});

test("next step: arm, re-check with the remembered exit, or wait for the user", () => {
  const base = { protect: true, busy: false, now: 100_000, notBefore: 0 };
  assert.equal(
    nextStep({ ...base, protect: false, status: status() }).kind,
    "wait",
  );
  assert.equal(nextStep({ ...base, status: null }).kind, "wait");
  assert.equal(
    nextStep({ ...base, busy: true, status: status() }).kind,
    "wait",
  );
  assert.equal(nextStep({ ...base, status: status() }).kind, "arm");
  assert.equal(nextStep({ ...base, status: allowed() }).kind, "wait");
  assert.equal(
    nextStep({ ...base, status: status({ checking: true }) }).kind,
    "wait",
  );
  // 出口变了：只能使用者重新放行。
  assert.equal(
    nextStep({ ...base, status: status({ armed: true, code: "ip_changed" }) })
      .kind,
    "user",
  );
  for (const code of [
    "ip_unknown",
    "stale",
    "manual_paused",
    "config_changed",
    "probe_timeout",
    "panel_stopping",
  ])
    assert.equal(
      nextStep({ ...base, status: status({ armed: true, code }) }).kind,
      "rearm",
      code,
    );
  // 没放行：退避期间等着，到点再核。
  const waiting = nextStep({
    ...base,
    notBefore: 115_000,
    status: status({ code: "ip_not_allowed" }),
  });
  assert.deepEqual(waiting, { kind: "wait", at: 115_000 });
  assert.ok(armBackoff(1) >= 15_000, "协议要求自动重核至少间隔 15 秒");
  assert.ok(armBackoff(1) < armBackoff(2) && armBackoff(9) === armBackoff(3));
  assert.ok(reconnectBackoff(1) <= 1_000 && reconnectBackoff(99) === 30_000);
});

test("protected sites: domains with subdomains, never IPs, localhost or single labels", () => {
  assert.equal(normalizeSite("claude.ai"), "claude.ai");
  assert.equal(normalizeSite(" https://Claude.AI/new?x=1 "), "claude.ai");
  assert.equal(normalizeSite("*.anthropic.com"), "anthropic.com");
  assert.equal(normalizeSite("http://example.com:8080"), "example.com");
  assert.equal(normalizeSite("例子.测试"), "xn--fsqu00a.xn--0zwm56d");
  for (const bad of [
    "",
    "ai",
    "localhost",
    "foo.localhost",
    "127.0.0.1",
    "[::1]",
    "http://user@claude.ai",
    "ftp://claude.ai",
    "-bad.com",
    "a..b",
    "x".repeat(400),
    "1.2.3",
    null,
  ])
    assert.equal(normalizeSite(bad), null, String(bad));
  assert.deepEqual(
    normalizeSites([
      "www.claude.ai",
      "claude.ai",
      "Anthropic.com",
      "claude.ai",
    ]),
    ["anthropic.com", "claude.ai"],
  );
  assert.throws(
    () => normalizeSites(["claude.ai", "127.0.0.1"]),
    /127\.0\.0\.1/,
  );
  assert.throws(
    () => normalizeSites(Array.from({ length: 51 }, (_, i) => `s${i}.test`)),
    /50/,
  );
  assert.equal(
    protectedSite("https://www.claude.ai/chat/1", DEFAULT_SITES),
    "claude.ai",
  );
  assert.equal(
    protectedSite("https://claude.ai.evil.test/", DEFAULT_SITES),
    null,
  );
  assert.equal(protectedSite("https://notclaude.ai/", DEFAULT_SITES), null);
  assert.equal(
    protectedSite("chrome-extension://x/blocked.html", DEFAULT_SITES),
    null,
  );
  assert.deepEqual(sitePatterns(["claude.ai"]), ["*://*.claude.ai/*"]);
});

test("block rules survive restarts (dynamic) and the allow rule outranks them (session)", () => {
  const [block] = blockRules(DEFAULT_SITES);
  const [allow] = allowRules(DEFAULT_SITES);
  assert.equal(block.id, BLOCK_RULE_ID);
  assert.equal(allow.id, ALLOW_RULE_ID);
  assert.equal(block.action.type, "block");
  assert.equal(allow.action.type, "allow");
  assert.ok(allow.priority > block.priority);
  for (const rule of [block, allow]) {
    assert.deepEqual(rule.condition.requestDomains, [...DEFAULT_SITES]);
    // 不写 resourceTypes 时 Chrome 不管主框架：页面本身就拦不住了。
    assert.ok(rule.condition.resourceTypes.includes("main_frame"));
    assert.deepEqual(rule.condition.resourceTypes, [...RESOURCE_TYPES]);
  }
  assert.deepEqual(blockRules([]), []);
  assert.deepEqual(allowRules([]), []);
});

test("the blocked page only ever returns to a protected http(s) address", () => {
  const page = blockedPage(
    "chrome-extension://id/blocked.html",
    "https://claude.ai/new?a=1#x",
  );
  const search = new URL(page).search;
  assert.equal(
    returnTarget(search, DEFAULT_SITES),
    "https://claude.ai/new?a=1#x",
  );
  for (const from of [
    "https://evil.test/",
    "javascript:alert(1)",
    "file:///C:/x",
  ])
    assert.equal(
      returnTarget(`?from=${encodeURIComponent(from)}`, DEFAULT_SITES),
      null,
    );
  assert.equal(returnTarget("", DEFAULT_SITES), null);
});

test("the popup shows one status and one main button for every state", () => {
  const base = {
    paired: true,
    link: "connected",
    linkReason: "",
    protect: false,
    allowed: false,
    verifying: false,
    status: status(),
    retryInMs: 0,
  };
  const primary = (state) =>
    describe({ ...base, ...state }).primary?.action ?? null;
  assert.equal(primary({ paired: false, link: "unpaired" }), "guide");
  assert.equal(primary({ link: "unavailable", linkReason: "连不上" }), "retry");
  assert.equal(primary({ link: "connecting" }), null);
  assert.equal(primary({}), "enable");
  assert.equal(
    primary({ protect: true, allowed: true, status: allowed() }),
    null,
  );
  assert.equal(primary({ protect: true, verifying: true }), null);
  assert.equal(
    primary({
      protect: true,
      status: status({ armed: true, code: "ip_changed" }),
    }),
    "reallow",
  );
  assert.equal(
    primary({ protect: true, status: status({ armed: true, code: "stale" }) }),
    "retry",
  );
  // 保护开着时随时能暂停；拦着时标题直接说「已拦截」。
  const blocked = describe({ ...base, protect: true, link: "unavailable" });
  assert.match(blocked.title, /拦截/);
  assert.deepEqual(blocked.secondary, [{ action: "pause", label: "暂停保护" }]);
  assert.match(
    describe({
      ...base,
      protect: true,
      status: status({
        armed: true,
        code: "ip_changed",
        ip: "1.1.1.2",
        baseline: "1.1.1.1",
      }),
    }).detail,
    /原来 1\.1\.1\.1，现在 1\.1\.1\.2/,
  );
  assert.equal(offlineReason({ code: "panel_exit" }), "QB Gate 面板已退出");
  assert.equal(offlineReason({ code: "x", reason: "a\nb" }), "a b");
});

test("manifest: fixed public ID, minimal permissions, every referenced file exists", () => {
  const manifest = JSON.parse(
    readFileSync(resolve(here, "manifest.json"), "utf8"),
  );
  const id = [
    ...createHash("sha256")
      .update(Buffer.from(manifest.key, "base64"))
      .digest("hex")
      .slice(0, 32),
  ]
    .map((c) => String.fromCharCode(97 + Number.parseInt(c, 16)))
    .join("");
  assert.equal(id, EXTENSION_ID);
  assert.equal(manifest.manifest_version, 3);
  assert.deepEqual([...manifest.permissions].sort(), [
    "activeTab",
    "alarms",
    "cookies",
    "declarativeNetRequest",
    "privacy",
    "proxy",
    "scripting",
    "storage",
  ]);
  assert.deepEqual(manifest.host_permissions, [
    "http://127.0.0.1/*",
    ...sitePatterns(DEFAULT_SITES),
  ]);
  assert.equal(
    manifest.web_accessible_resources,
    undefined,
    "拦截页不对网页开放",
  );
  const files = [
    manifest.background.service_worker,
    manifest.action.default_popup,
    manifest.options_ui.page,
    ...manifest.content_scripts.flatMap((script) => script.js),
    ...Object.values(manifest.icons),
    ...Object.values(manifest.action.default_icon),
    "welcome.html",
    "blocked.html",
  ];
  for (const file of files) assert.ok(existsSync(resolve(here, file)), file);
  // 页面引用的脚本与样式也都在。
  for (const page of [
    "popup.html",
    "welcome.html",
    "options.html",
    "blocked.html",
  ]) {
    const html = readFileSync(resolve(here, page), "utf8");
    for (const [, ref] of html.matchAll(/(?:src|href)="([^"#:]+)"/g))
      assert.ok(existsSync(resolve(here, ref)), `${page} → ${ref}`);
  }
});

test("the extension's colours are the panel's tokens.css, byte for byte", () => {
  assert.equal(
    readFileSync(resolve(here, "tokens.css"), "utf8"),
    readFileSync(resolve(here, "../src/styles/tokens.css"), "utf8"),
  );
});

test("probe urls must be the panel's one-off loopback page", () => {
  assert.equal(validProbeUrl(`http://127.0.0.1:4000/${"a".repeat(32)}/`), true);
  for (const url of [
    `http://localhost:4000/${"a".repeat(32)}/`,
    `http://127.0.0.1:4000/${"a".repeat(32)}/?x`,
    `https://127.0.0.1:4000/${"a".repeat(32)}/`,
    "http://127.0.0.1:4000/x/",
  ])
    assert.equal(validProbeUrl(url), false, url);
});

test("station bindings compare exact origins including scheme and port", () => {
  assert.equal(
    sameStation("https://billing.test/path", "https://billing.test"),
    true,
  );
  for (const url of [
    "http://billing.test",
    "https://billing.test.evil.test",
    "https://billing.test:8443",
    "https://user@billing.test",
    "chrome://settings",
  ])
    assert.equal(sameStation(url, "https://billing.test"), false);
  assert.equal(
    sameStation("https://billing.test", "https://billing.test/path"),
    false,
  );
});

test("capture recognizes Sub2 auth keys and strips passwords, refresh tokens, unrelated storage and nested secrets", () => {
  const values = {
    auth_token: "fixture-access",
    auth_user: JSON.stringify({
      id: 7,
      email: "fixture@example.invalid",
      password: "must-not-leave",
    }),
    auth: JSON.stringify({
      data: {
        access_token: "fixture-auth",
        refresh_token: "must-not-leave",
        user: { id: 7, username: "fixture", api_key: "must-not-leave" },
      },
      password: "must-not-leave",
    }),
    refresh_token: "must-not-leave",
    unrelated: "must-not-leave",
    token: "bad\r\nheader",
  };
  const reads = [];
  const storage = {
    getItem: (key) => {
      reads.push(key);
      return values[key] ?? null;
    },
  };
  const result = vm.runInNewContext(
    `(${captureCurrentAccount.toString()})("https://billing.test")`,
    {
      location: { origin: "https://billing.test" },
      localStorage: storage,
      sessionStorage: { getItem: () => null },
    },
  );
  const text = JSON.stringify(result);
  assert.match(text, /fixture-access/);
  assert.match(text, /fixture@example.invalid/);
  assert.doesNotMatch(
    text,
    /must-not-leave|refresh_token|password|api_key|bad/,
  );
  assert.ok(!reads.includes("refresh_token") && !reads.includes("unrelated"));
  assert.throws(() =>
    vm.runInNewContext(
      `(${captureCurrentAccount.toString()})("https://other.test")`,
      {
        location: { origin: "https://billing.test" },
      },
    ),
  );
});

test("settings summaries never include PAC, proxy host or credentials", () => {
  const result = enhancementSummary(
    { value: "disable_non_proxied_udp" },
    {
      value: {
        mode: "pac_script",
        pacScript: { data: "secret PAC" },
        password: "secret",
      },
      levelOfControl: "controlled_by_other_extensions",
    },
  );
  assert.deepEqual(result, {
    webrtc_policy: "disable_non_proxied_udp",
    proxy_mode: "pac_script",
    proxy_control: "controlled_by_other_extensions",
  });
  assert.deepEqual(enhancementSummary(null, null), {
    webrtc_policy: null,
    proxy_mode: null,
    proxy_control: null,
  });
});

test("cookie capture accepts only one same-origin session and rejects ambiguous partitions", () => {
  const cookie = { name: "session", value: "fixture", domain: "billing.test" };
  assert.equal(
    sessionCookie(
      [
        cookie,
        { ...cookie, name: "analytics" },
        { ...cookie, domain: "elsewhere.test" },
      ],
      "https://billing.test",
    ),
    "session=fixture",
  );
  assert.throws(() =>
    sessionCookie(
      [cookie, { ...cookie, value: "another-partition" }],
      "https://billing.test",
    ),
  );
  assert.equal(
    sessionCookie(
      [{ ...cookie, value: "bad; extra=1" }],
      "https://billing.test",
    ),
    "",
  );
});
