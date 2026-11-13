import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import {
  Protection,
  STALE_MS,
  enhancementSummary,
  parsePairing,
  sameStation,
  sessionCookie,
} from "./core.js";
import { captureCurrentAccount } from "./capture.js";

const healthy = {
  running: true,
  connected: true,
  armed: true,
  safe: true,
  ip: "203.0.113.10",
  reason: "allowed",
  checked_at: 123,
};
test("pairing rejects remote hosts, DNS aliases, credentials, paths and weak secrets", () => {
  const pair = (endpoint, token = "a".repeat(64)) =>
    JSON.stringify({ endpoint, token });
  assert.deepEqual(parsePairing(pair("http://127.0.0.1:27189")), {
    endpoint: "http://127.0.0.1:27189",
    token: "a".repeat(64),
  });
  for (const endpoint of [
    "https://example.test:27189",
    "http://localhost:27189",
    "http://127.0.0.1.evil.test:27189",
    "http://user@127.0.0.1:27189",
    "http://127.0.0.1:27189/a",
    "http://127.0.0.1:27189?token=a",
    "http://127.0.0.1:27189#x",
    "http://127.0.0.1",
  ])
    assert.throws(() => parsePairing(pair(endpoint)));
  assert.throws(() => parsePairing(pair("http://127.0.0.1:27189", "short")));
});
test("no close decision before opt-in; every unknown/changed/expired state closes after opt-in", () => {
  const p = new Protection();
  assert.equal(p.accept({ ...healthy, safe: false }, 1), null);
  assert.equal(p.disconnected(), null);
  assert.equal(p.stale(100_000), null);
  assert.throws(() => p.arm({ ...healthy, safe: false }, 0));
  p.arm(healthy, 100);
  assert.equal(p.accept(healthy, 150), null);
  assert.match(p.accept({ ...healthy, ip: "203.0.113.11" }, 200), /变化/);
  assert.match(
    p.accept({ ...healthy, ip: null, safe: false, reason: "unknown" }, 250),
    /unknown/,
  );
  assert.match(
    p.accept({ ...healthy, armed: false, reason: "manual stop" }, 300),
    /manual stop/,
  );
  assert.equal(p.stale(300 + STALE_MS - 1), null);
  assert.match(p.stale(300 + STALE_MS), /超时/);
  assert.match(p.accept({ safe: true }, 400), /无效/);
  p.disarm();
  assert.equal(p.disconnected(), null);
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
      { location: { origin: "https://billing.test" } },
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
