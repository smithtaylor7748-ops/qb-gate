// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 插件后台（background.js）的单测：假的 chrome.*、假的桥（证明用真的 HMAC）、可以拨动的两个时钟。
// CI 里跑不了真实浏览器，后台的取舍在这里钉住；真实浏览器里的行为见 scripts/browser-extension-e2e.mjs。
import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";
import * as core from "./core.js";
import { captureCurrentAccount } from "./capture.js";

const ID = "dffmajnemniojfblgfhenndeeelbjgoh";
const ORIGIN = `chrome-extension://${ID}`;
const TOKEN = "0123456789abcdef".repeat(4);
const PAIRING = "0123456789abcdef";
const PORT = 41234;
const IP_A = "203.0.113.10";
const IP_B = "203.0.113.11";
const source = readFileSync(
  new URL("./background.js", import.meta.url),
  "utf8",
).replace(/^import[\s\S]*?from "[^"]+";\r?\n/gm, "");

const settle = async (rounds = 40) => {
  for (let i = 0; i < rounds; i++)
    await new Promise((done) => setImmediate(done));
};

/** 手动拨的时钟：monotonic 给 performance.now，wall 给 Date.now；sleep 只拨墙上时钟（睡眠）。 */
function makeClock() {
  let mono = 0;
  let wall = 1_700_000_000_000;
  let next = 0;
  const timers = new Map();
  const schedule = (id, fn, ms, repeat) =>
    timers.set(id, { at: mono + Math.max(0, ms || 0), fn, repeat, ms });
  const FakeDate = class extends Date {
    constructor(...args) {
      if (args.length) super(...args);
      else super(wall + mono);
    }
    static now() {
      return wall + mono;
    }
  };
  return {
    Date: FakeDate,
    performance: { now: () => mono },
    setTimeout: (fn, ms) => (schedule(++next, fn, ms, false), next),
    setInterval: (fn, ms) => (schedule(++next, fn, ms, true), next),
    clearTimeout: (id) => timers.delete(id),
    clearInterval: (id) => timers.delete(id),
    wall: () => wall + mono,
    sleep(ms) {
      wall += ms;
    },
    async advance(ms) {
      const target = mono + ms;
      for (;;) {
        const due = [...timers]
          .filter(([, t]) => t.at <= target)
          .sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        const [id, timer] = due;
        mono = timer.at;
        if (timer.repeat) schedule(id, timer.fn, timer.ms, true);
        else timers.delete(id);
        timer.fn();
        await settle();
      }
      mono = target;
      await settle();
    },
  };
}

/** 假的本机连接桥：握手证明、状态推送、/v2/* 与 /link/claim。 */
function makeBridge(clock) {
  const bridge = {
    port: PORT,
    serverToken: TOKEN,
    down: false,
    sockets: [],
    requests: [],
    seq: 1,
    ip: IP_A,
    onArm: null,
    onClaim: null,
    last: null,
    silent: false,
    status(extra = {}) {
      return {
        type: "status",
        protocol: 2,
        seq: bridge.seq,
        allow: false,
        code: "not_armed",
        reason: "插件已连接，保护尚未开启",
        armed: false,
        checking: false,
        ip: null,
        baseline: null,
        checked_at: Math.floor(clock.wall() / 1000),
        ttl_ms: 10_000,
        ...extra,
      };
    },
    allow(extra = {}) {
      return bridge.status({
        allow: true,
        code: "allowed",
        reason: "出口 IP 已核实，可以放行",
        armed: true,
        ip: bridge.ip,
        baseline: bridge.ip,
        ...extra,
      });
    },
    get socket() {
      return bridge.sockets.at(-1);
    },
    /** 记下「现在的状态」：心跳照它原样再发（晚到的旧回包不算）。 */
    remember(value) {
      if (!bridge.last || value.seq >= bridge.last.seq) bridge.last = value;
      return value;
    },
    push(value) {
      bridge.seq = Math.max(bridge.seq, value.seq ?? bridge.seq);
      bridge.socket.emit(bridge.remember(value));
    },
    /** 跟真的桥一样：恢复保护时带的出口跟现在的不一样，就是「出口变了」。 */
    arm(body) {
      if (body?.baseline && body.baseline !== bridge.ip)
        return bridge.status({
          armed: true,
          code: "ip_changed",
          reason: "出口 IP 已变化",
          ip: bridge.ip,
          baseline: body.baseline,
        });
      return bridge.allow();
    },
    async receive(socket, message) {
      if (message.type === "hello") {
        socket.n1 = message.nonce;
        socket.n2 = "e".repeat(64);
        socket.emit({
          type: "challenge",
          protocol: 2,
          nonce: socket.n2,
          proof: await core.proof(
            bridge.serverToken,
            "server",
            socket.n1,
            socket.n2,
          ),
        });
      } else if (message.type === "authenticate") {
        const expected = await core.proof(
          TOKEN,
          "client",
          socket.n1,
          socket.n2,
        );
        if (message.proof !== expected)
          return socket.emit({ type: "rejected", code: "bad_proof" });
        socket.session = "5".repeat(64);
        socket.emit({
          type: "welcome",
          protocol: 2,
          connection: 1,
          pairing: PAIRING,
          session: socket.session,
          port: bridge.port,
        });
        bridge.seq += 1;
        socket.emit(bridge.remember(bridge.status()));
      }
    },
    async fetch(url, init = {}) {
      const { pathname, port } = new URL(url);
      const body = init.body ? JSON.parse(init.body) : undefined;
      bridge.requests.push({ path: pathname, body, port: Number(port) });
      const reply = (value, status = 200) =>
        new Response(JSON.stringify(value), { status });
      if (bridge.down) throw new TypeError("connection refused");
      if (pathname === "/link/claim")
        return bridge.onClaim(body, Number(port), reply);
      if (pathname === "/v2/disarm") {
        bridge.seq += 1;
        return reply(
          bridge.remember(bridge.status({ reason: "已暂停浏览器保护" })),
        );
      }
      if (pathname === "/v2/arm") {
        bridge.seq += 1;
        const value = bridge.onArm
          ? await bridge.onArm(body)
          : bridge.arm(body);
        return reply(bridge.remember(value));
      }
      return reply({ error: "not found" }, 404);
    },
  };
  bridge.WebSocket = class FakeSocket {
    static OPEN = 1;
    constructor(url) {
      this.url = url;
      this.readyState = 0;
      this.sent = [];
      bridge.sockets.push(this);
      setImmediate(() => {
        if (bridge.down || Number(new URL(url).port) !== bridge.port) {
          this.readyState = 3;
          this.onclose?.();
        } else {
          this.readyState = 1;
          this.onopen?.();
        }
      });
    }
    send(text) {
      const message = JSON.parse(text);
      this.sent.push(message);
      void bridge.receive(this, message);
    }
    emit(value) {
      if (this.readyState === 1)
        this.onmessage?.({ data: JSON.stringify(value) });
    }
    close() {
      if (this.readyState === 3) return;
      this.readyState = 3;
      setImmediate(() => this.onclose?.());
    }
  };
  // 心跳：每 2 秒把现在的状态原样再发一次（silent 时不发，模拟面板卡住）。
  clock.setInterval(() => {
    const socket = bridge.socket;
    if (
      !bridge.silent &&
      socket?.readyState === 1 &&
      socket.session &&
      bridge.last
    )
      socket.emit(bridge.last);
  }, 2_000);
  return bridge;
}

async function boot({
  local = {},
  session = [],
  dynamic = [],
  tabs = [],
  before,
} = {}) {
  const clock = makeClock();
  const bridge = makeBridge(clock);
  before?.(bridge);
  const log = [];
  const listeners = {};
  const on = (name) => ({
    addListener: (fn) => (listeners[name] ??= []).push(fn),
  });
  const rules = {
    dynamic: structuredClone(dynamic),
    session: structuredClone(session),
  };
  const store = structuredClone(local);
  const env = { badge: "" };
  const chrome = {
    runtime: {
      id: ID,
      getManifest: () => ({ version: "0.2.0" }),
      getURL: (path) => `${ORIGIN}/${path.replace(/^\//, "")}`,
      onInstalled: on("installed"),
      onMessage: on("message"),
      onSuspend: on("suspend"),
      openOptionsPage: async () => log.push(["options"]),
      getPlatformInfo: async () => ({}),
    },
    storage: {
      local: {
        get: async (keys) =>
          Object.fromEntries(
            []
              .concat(keys)
              .filter((k) => k in store)
              .map((k) => [k, structuredClone(store[k])]),
          ),
        set: async (value) => Object.assign(store, structuredClone(value)),
        remove: async (keys) => [].concat(keys).forEach((k) => delete store[k]),
      },
    },
    declarativeNetRequest: {
      getDynamicRules: async () => structuredClone(rules.dynamic),
      getSessionRules: async () => structuredClone(rules.session),
      updateDynamicRules: async ({ removeRuleIds = [], addRules = [] }) => {
        rules.dynamic = rules.dynamic
          .filter((r) => !removeRuleIds.includes(r.id))
          .concat(structuredClone(addRules));
        log.push([
          "dynamic",
          rules.dynamic.map((r) => r.action.type).join(","),
        ]);
      },
      updateSessionRules: async ({ removeRuleIds = [], addRules = [] }) => {
        rules.session = rules.session
          .filter((r) => !removeRuleIds.includes(r.id))
          .concat(structuredClone(addRules));
        log.push([
          "session",
          rules.session.map((r) => r.action.type).join(","),
        ]);
      },
    },
    tabs: {
      query: async (query) =>
        structuredClone(query.active ? tabs.filter((t) => t.active) : tabs),
      update: async (id, { url }) => {
        const tab = tabs.find((t) => t.id === id);
        if (tab) tab.url = url;
        log.push(["navigate", id, url]);
      },
      create: async ({ url }) => log.push(["create", url]),
      onUpdated: on("tabUpdated"),
    },
    action: {
      setBadgeText: async ({ text }) => (env.badge = text),
      setBadgeBackgroundColor: async () => {},
      setTitle: async () => {},
    },
    alarms: {
      get: async () => undefined,
      create: async () => {},
      onAlarm: on("alarm"),
    },
    proxy: {
      settings: {
        get: async () => ({ value: { mode: "system" } }),
        onChange: on("proxy"),
      },
    },
    privacy: {
      network: {
        webRTCIPHandlingPolicy: { get: async () => ({ value: "default" }) },
      },
    },
    permissions: { contains: async () => true },
  };
  const context = vm.createContext({
    ...core,
    captureCurrentAccount,
    chrome,
    WebSocket: bridge.WebSocket,
    fetch: (url, init) => bridge.fetch(url, init),
    navigator: {},
    AbortSignal,
    Response,
    TextEncoder,
    TextDecoder,
    URL,
    URLSearchParams,
    console,
    structuredClone,
    Date: clock.Date,
    performance: clock.performance,
    setTimeout: clock.setTimeout,
    setInterval: clock.setInterval,
    clearTimeout: clock.clearTimeout,
    clearInterval: clock.clearInterval,
  });
  vm.runInContext(source, context);
  await settle(80);
  const send = (message, sender = { id: ID, url: `${ORIGIN}/popup.html` }) =>
    new Promise((done) => {
      const handled = listeners.message[0](message, sender, done);
      if (!handled) done("ignored");
    });
  const state = async () => (await send({ type: "state" })).value;
  return {
    clock,
    bridge,
    log,
    listeners,
    rules,
    store,
    env,
    tabs,
    send,
    state,
  };
}

const paired = (extra = {}) => ({
  pairing: { id: PAIRING, token: TOKEN, port: PORT },
  sites: [...core.DEFAULT_SITES],
  ...extra,
});
const ruleTypes = (rules) => ({
  dynamic: rules.dynamic.map((r) => r.action.type),
  session: rules.session.map((r) => r.action.type),
});

test("waking up removes a leftover allow rule before anything else; protection without a pairing only blocks", async () => {
  const t = await boot({
    local: { protect: true },
    session: core.allowRules(core.DEFAULT_SITES),
  });
  assert.deepEqual(t.log[0], ["session", ""], "第一件事是撤放行");
  assert.deepEqual(ruleTypes(t.rules), { dynamic: ["block"], session: [] });
  assert.equal(t.env.badge, "拦");
  const s = await t.state();
  assert.equal(s.paired, false);
  assert.equal(s.allowed, false);
});

test("the handshake never proves itself to a port that cannot prove first", async () => {
  // 头一次：端口上的程序不认得口令（证明不对）。
  const t = await boot({
    local: paired(),
    before: (bridge) => (bridge.serverToken = "f".repeat(64)),
  });
  await t.clock.advance(0);
  const impostor = t.bridge.sockets[0];
  assert.deepEqual(
    impostor.sent.map((m) => m.type),
    ["hello"],
    "证明不对就不该开口",
  );
  assert.equal(impostor.readyState, 3);
  assert.match((await t.state()).linkReason, /不是认得这个浏览器的 QB Gate/);
  // 换回真的桥：重连后插件给出正确的证明。
  t.bridge.serverToken = TOKEN;
  await t.clock.advance(1_000);
  const real = t.bridge.socket;
  assert.notEqual(real, impostor);
  const auth = real.sent.find((m) => m.type === "authenticate");
  assert.equal(auth.proof, await core.proof(TOKEN, "client", real.n1, real.n2));
  assert.equal((await t.state()).link, "connected");
});

test("allow installs the session rule after the block rule; a trip revokes it and moves only protected tabs", async () => {
  const tabs = [
    { id: 1, url: "https://claude.ai/chat/1" },
    { id: 2, url: "https://example.com/" },
  ];
  const t = await boot({
    local: paired({ protect: true, baseline: IP_A }),
    tabs,
  });
  await t.clock.advance(0);
  const arm = t.bridge.requests.find((r) => r.path === "/v2/arm");
  assert.deepEqual(arm.body, { baseline: IP_A }, "恢复保护时带上次放行的出口");
  assert.deepEqual(ruleTypes(t.rules), {
    dynamic: ["block"],
    session: ["allow"],
  });
  const addAllow = t.log.findIndex(
    ([kind, value]) => kind === "session" && value === "allow",
  );
  const addBlock = t.log.findIndex(
    ([kind, value]) => kind === "dynamic" && value === "block",
  );
  assert.ok(addBlock >= 0 && addBlock < addAllow, "先有拦截、后有放行");
  assert.equal((await t.state()).allowed, true);
  assert.equal(t.env.badge, "放");
  // 触发之后插件会立刻按上次放行的出口再核一次：让这一次也不放行。
  t.bridge.onArm = async () =>
    t.bridge.status({ code: "ip_unknown", reason: "查不到出口 IP" });
  t.bridge.seq += 1;
  t.bridge.push(
    t.bridge.status({
      armed: true,
      code: "ip_unknown",
      reason: "查不到出口 IP",
      ip: null,
      baseline: IP_A,
    }),
  );
  await settle(80);
  assert.deepEqual(ruleTypes(t.rules), { dynamic: ["block"], session: [] });
  const moved = t.log.filter(([kind]) => kind === "navigate");
  assert.deepEqual(
    moved.map(([, id]) => id),
    [1],
    "只动受保护网站的标签页",
  );
  assert.equal(
    moved[0][2],
    core.blockedPage(`${ORIGIN}/blocked.html`, "https://claude.ai/chat/1"),
  );
  assert.equal(t.tabs[1].url, "https://example.com/");
  assert.equal(t.env.badge, "拦");
});

test("a late reply with a smaller sequence number cannot undo a newer block", async () => {
  const t = await boot({
    local: paired({ protect: true, baseline: IP_A }),
    before: (bridge) =>
      (bridge.onArm = async () => {
        const reply = bridge.allow({ seq: bridge.seq });
        bridge.seq += 1;
        bridge.push(
          bridge.status({
            armed: true,
            code: "ip_unknown",
            reason: "查不到",
            baseline: IP_A,
          }),
        );
        await settle();
        return reply; // 晚到的「放行」，序号比上面那条推送小
      }),
  });
  await t.clock.advance(0);
  const s = await t.state();
  assert.equal(s.allowed, false);
  assert.equal(s.status.code, "ip_unknown");
  assert.deepEqual(t.rules.session, []);
});

test("an allow expires after ttl_ms without heartbeats, and after a sleep that only moves the wall clock", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  assert.equal((await t.state()).allowed, true);
  await t.clock.advance(30_000);
  assert.equal((await t.state()).allowed, true, "心跳在，放行一直有效");
  t.bridge.silent = true; // 桥不出声了（面板卡住），连接没断
  await t.clock.advance(9_900);
  assert.equal((await t.state()).allowed, true);
  await t.clock.advance(300);
  const held = await t.state();
  assert.equal(held.allowed, false, "放行到期");
  assert.equal(held.hold, "expired");
  assert.match(core.describe(held).title, /有一阵没回话/);
  assert.deepEqual(t.rules.session, []);
  // 心跳回来就续上；然后睡一觉：单调钟没走、墙上时钟跳了一分钟，醒来第一个闹钟就撤放行。
  t.bridge.silent = false;
  t.bridge.push(t.bridge.allow());
  await settle(80);
  assert.equal((await t.state()).allowed, true);
  t.clock.sleep(60_000);
  for (const fn of t.listeners.alarm) fn({ name: "qb-wake" });
  await settle(80);
  assert.equal((await t.state()).allowed, false);
  assert.deepEqual(t.rules.session, []);
});

test("a changed exit waits for the user; re-allow pauses then arms without the old exit", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  t.bridge.ip = IP_B;
  t.bridge.seq += 1;
  t.bridge.push(
    t.bridge.status({
      armed: true,
      code: "ip_changed",
      reason: "出口 IP 已变化",
      ip: IP_B,
      baseline: IP_A,
    }),
  );
  await settle(80);
  const before = t.bridge.requests.length;
  await t.clock.advance(120_000);
  assert.equal(t.bridge.requests.length, before, "出口变了不该自己去核");
  assert.equal((await t.send({ type: "reallow" })).ok, true);
  const calls = t.bridge.requests.slice(before).map((r) => [r.path, r.body]);
  assert.deepEqual(calls, [
    ["/v2/disarm", {}],
    ["/v2/arm", {}],
  ]);
  const s = await t.state();
  assert.equal(s.allowed, true);
  assert.equal(s.baseline, IP_B);
});

test("other refusals re-check with the remembered exit, backing off at least 15 seconds", async () => {
  const t = await boot({
    local: paired({ protect: true, baseline: IP_A }),
    before: (bridge) =>
      (bridge.onArm = async () =>
        bridge.status({
          code: "ip_unknown",
          reason: "查不到出口 IP",
          baseline: null,
        })),
  });
  await t.clock.advance(0);
  const arms = () => t.bridge.requests.filter((r) => r.path === "/v2/arm");
  assert.equal(arms().length, 1);
  await t.clock.advance(14_900);
  assert.equal(arms().length, 1, "15 秒之内不再探");
  await t.clock.advance(200);
  assert.equal(arms().length, 2);
  assert.deepEqual(arms()[1].body, { baseline: IP_A });
  t.bridge.onArm = null;
  await t.clock.advance(30_000);
  assert.equal((await t.state()).allowed, true);
});

test("offline keeps its reason through failed reconnects; revoked forgets the pairing; unproven unknown_pairing does not", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  t.bridge.down = true;
  t.bridge.socket.emit({
    type: "offline",
    protocol: 2,
    code: "panel_exit",
    reason: "x",
  });
  await settle(80);
  await t.clock.advance(3_000); // 重连失败（面板没开）
  let s = await t.state();
  assert.equal(s.link, "unavailable");
  assert.equal(s.linkReason, "QB Gate 面板已退出");
  assert.equal(s.allowed, false);
  t.bridge.down = false;
  await t.clock.advance(10_000);
  assert.equal((await t.state()).allowed, true, "面板回来后自己连上");
  // 未经证明的「不认识这个配对」：不删本地配对，60 秒后再试。
  t.bridge.socket.emit({
    type: "rejected",
    code: "unknown_pairing",
    reason: "?",
  });
  await settle(80);
  assert.ok(t.store.pairing, "没经过证明的拒绝不能删本地配对");
  const sockets = t.bridge.sockets.length;
  await t.clock.advance(59_000);
  assert.equal(t.bridge.sockets.length, sockets);
  await t.clock.advance(2_000);
  assert.equal(t.bridge.sockets.length, sockets + 1);
  // 已认证连接里的撤销：删本地配对，保护照样开着、照样拦。
  t.bridge.socket.emit({ type: "revoked", protocol: 2, reason: "解除" });
  await settle(80);
  s = await t.state();
  assert.equal(t.store.pairing, undefined);
  assert.equal(s.paired, false);
  assert.equal(s.protect, true);
  assert.match(s.linkReason, /解除/);
  assert.deepEqual(ruleTypes(t.rules), { dynamic: ["block"], session: [] });
});

test("claims come only from the claim page; relink proves with the stored token; an unknown pairing re-claims as new", async () => {
  const t = await boot({ local: paired() });
  await t.clock.advance(0);
  const code = "c".repeat(64);
  const tab = (url) => ({ id: ID, url, tab: { id: 9 }, frameId: 0 });
  assert.equal(
    await t.send(
      { type: "claim", claim: code },
      tab("http://127.0.0.1:41999/other"),
    ),
    "ignored",
  );
  assert.equal(
    await t.send(
      { type: "claim", claim: code },
      { ...tab("https://evil.test/link"), id: "x" },
    ),
    "ignored",
  );
  assert.equal(
    await t.send({ type: "state" }, tab("http://127.0.0.1:41999/link")),
    "ignored",
  );
  const bodies = [];
  t.bridge.onClaim = async (body, port, reply) => {
    bodies.push(body);
    return reply({
      type: "relinked",
      protocol: 2,
      pairing: PAIRING,
      port,
      proof: await core.proof(TOKEN, "claimed", PAIRING, code),
    });
  };
  t.bridge.port = 41999;
  const relink = await t.send(
    { type: "claim", claim: code },
    tab(`http://127.0.0.1:41999/link#c=${code}`),
  );
  assert.equal(relink.ok, true, relink.error);
  assert.equal(bodies[0].pairing, PAIRING);
  assert.equal(
    bodies[0].proof,
    await core.proof(TOKEN, "claim", PAIRING, code),
  );
  assert.equal(t.store.pairing.port, 41999);
  await t.clock.advance(0);
  assert.equal((await t.state()).link, "connected");
  // 面板里解除过：桥回 unknown_pairing（不消耗认领码），插件当新浏览器再认领一次。
  const fresh = "9".repeat(64);
  bodies.length = 0;
  t.bridge.onClaim = async (body, port, reply) => {
    bodies.push(body);
    if (body.pairing)
      return reply({ error: "不认识", code: "unknown_pairing" }, 404);
    return reply({
      type: "paired",
      protocol: 2,
      pairing: "fedcba9876543210",
      token: fresh,
      port,
      proof: await core.proof(fresh, "claimed", "fedcba9876543210", code),
    });
  };
  const again = await t.send(
    { type: "claim", claim: code },
    tab(`http://127.0.0.1:41999/link#c=${code}`),
  );
  assert.equal(again.ok, true, again.error);
  assert.equal(bodies.length, 2);
  assert.equal(bodies[1].pairing, undefined);
  assert.deepEqual(t.store.pairing, {
    id: "fedcba9876543210",
    token: fresh,
    port: 41999,
  });
  // 证明对不上的回包不保存。
  t.bridge.onClaim = async (body, port, reply) =>
    reply({
      type: "paired",
      protocol: 2,
      pairing: "1111111111111111",
      token: "2".repeat(64),
      port,
      proof: "0".repeat(64),
    });
  const forged = await t.send(
    { type: "claim", claim: code },
    tab(`http://127.0.0.1:41999/link#c=${code}`),
  );
  assert.equal(forged.ok, false);
  assert.equal(t.store.pairing.id, "fedcba9876543210");
});

test("pause removes every rule and disarms; enable forgets the old exit and verifies again", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  assert.equal((await t.send({ type: "pause" })).ok, true);
  assert.deepEqual(ruleTypes(t.rules), { dynamic: [], session: [] });
  assert.equal(t.bridge.requests.at(-1).path, "/v2/disarm");
  assert.equal(t.store.protect, false);
  assert.equal(t.store.baseline, null);
  assert.equal(t.env.badge, "");
  assert.equal((await t.send({ type: "enable" })).ok, true);
  assert.deepEqual(t.bridge.requests.at(-1), {
    path: "/v2/arm",
    body: {},
    port: PORT,
  });
  assert.deepEqual(ruleTypes(t.rules), {
    dynamic: ["block"],
    session: ["allow"],
  });
});

test("a proxy change distrusts the current allow until the bridge re-checks", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  assert.equal((await t.state()).allowed, true);
  await t.clock.advance(2_000);
  for (const fn of t.listeners.proxy) fn({ value: { mode: "fixed_servers" } });
  await settle(80);
  assert.equal((await t.state()).allowed, false);
  assert.ok(
    t.bridge.socket.sent.some((m) => m.type === "recheck"),
    "请桥立刻复查",
  );
  // 复查之前的心跳（旧的核实时间）不算；复查出来的新结果才算。
  t.bridge.push(
    t.bridge.allow({ checked_at: Math.floor(t.clock.wall() / 1000) - 5 }),
  );
  await settle(80);
  assert.equal((await t.state()).allowed, false);
  await t.clock.advance(1_000);
  t.bridge.seq += 1;
  t.bridge.push(t.bridge.allow());
  await settle(80);
  assert.equal((await t.state()).allowed, true);
});

test("protected sites can be changed only to valid domains, and the rules follow at once", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  const bad = await t.send({
    type: "sites",
    sites: ["claude.ai", "127.0.0.1"],
  });
  assert.equal(bad.ok, false);
  assert.match(bad.error, /127\.0\.0\.1/);
  assert.equal((await t.send({ type: "sites", sites: [] })).ok, false);
  const ok = await t.send({
    type: "sites",
    sites: ["claude.ai", "example.org"],
  });
  assert.deepEqual(ok.value, ["claude.ai", "example.org"]);
  assert.deepEqual(t.rules.dynamic[0].condition.requestDomains, [
    "claude.ai",
    "example.org",
  ]);
  assert.deepEqual(t.rules.session[0].condition.requestDomains, [
    "claude.ai",
    "example.org",
  ]);
  assert.equal(
    await t.send(
      { type: "state" },
      { id: "another-extension", url: `${ORIGIN}/popup.html` },
    ),
    "ignored",
  );
});

test("a connection that stays silent for 15 seconds is dropped and re-established", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  const first = t.bridge.socket;
  t.bridge.silent = true;
  await t.clock.advance(14_000);
  assert.equal(t.bridge.socket, first, "15 秒之内不断");
  assert.equal((await t.state()).hold, "expired");
  await t.clock.advance(6_100);
  assert.equal(first.readyState, 3, "一直没回话的连接被丢掉");
  await t.clock.advance(1_500);
  assert.notEqual(t.bridge.socket, first, "重新连上");
  t.bridge.silent = false;
  await t.clock.advance(1_000);
  const s = await t.state();
  assert.equal(s.link, "connected");
  assert.equal(s.allowed, true, "同一个出口：按上次放行的出口核过就放行");
});

test("after reconnecting, a different exit than the remembered one is never allowed silently", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  t.bridge.ip = IP_B;
  t.bridge.socket.close(); // 面板那边断开（比如重启）
  await t.clock.advance(0);
  await t.clock.advance(2_000);
  const s = await t.state();
  assert.equal(s.link, "connected");
  assert.equal(s.allowed, false);
  assert.equal(s.status.code, "ip_changed");
  assert.equal(core.describe(s).primary.action, "reallow");
});

test("「重试连接」while the bridge is silent drops the stale connection right away", async () => {
  const t = await boot({ local: paired({ protect: true, baseline: IP_A }) });
  await t.clock.advance(0);
  const first = t.bridge.socket;
  t.bridge.silent = true;
  await t.clock.advance(11_000);
  assert.equal((await t.state()).hold, "expired");
  t.bridge.silent = false;
  assert.equal((await t.send({ type: "retry" })).ok, true);
  await t.clock.advance(0);
  assert.equal(first.readyState, 3);
  await t.clock.advance(1_500);
  assert.notEqual(t.bridge.socket, first);
  assert.equal((await t.state()).allowed, true);
});
