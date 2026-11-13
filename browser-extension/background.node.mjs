import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";
import * as core from "./core.js";
import { captureCurrentAccount } from "./capture.js";

const source = readFileSync(
  new URL("./background.js", import.meta.url),
  "utf8",
).replace(/^import[\s\S]*?from "[^"]+";\r?\n/gm, "");
const status = (extra = {}) => ({
  type: "status",
  running: true,
  connected: true,
  armed: false,
  safe: false,
  ip: null,
  reason: "idle",
  checked_at: 100,
  enhancement_json: null,
  ...extra,
});
const flush = async () => {
  for (let i = 0; i < 12; i++)
    await new Promise((resolve) => setImmediate(resolve));
};

async function fixture(options = {}) {
  const stored = {
    pairing: { endpoint: "http://127.0.0.1:27189", token: "a".repeat(64) },
  };
  const session = { protection: options.protection ?? { armed: false } };
  const events = {};
  const network = {
    type: "wifi",
    rtt: 20,
    downlink: 10,
    effectiveType: "4g",
    addEventListener: (name, fn) => {
      events.network = fn;
    },
  };
  const messages = [];
  const removed = [],
    timers = [],
    sockets = [],
    requests = [];
  let pendingAuth,
    finishArm,
    failedRemoval = false;
  let handler,
    clock = 100,
    server = status(),
    scripts = 0;
  const event = () => ({ addListener() {} });
  class Socket {
    static OPEN = 1;
    readyState = 1;
    constructor() {
      sockets.push(this);
      queueMicrotask(() => this.onopen?.());
    }
    send(text) {
      messages.push(JSON.parse(text));
      if (JSON.parse(text).type === "authenticate") {
        if (options.delayConnect) pendingAuth = () => this.emit(server);
        else queueMicrotask(() => this.emit(server));
      }
    }
    emit(value) {
      this.onmessage?.({ data: JSON.stringify(value) });
    }
    close() {
      this.readyState = 3;
      this.onclose?.();
    }
  }
  const chrome = {
    runtime: {
      id: "a".repeat(32),
      getURL: (path) => `chrome-extension://${"a".repeat(32)}/${path}`,
      onMessage: {
        addListener: (fn) => {
          handler = fn;
        },
      },
    },
    storage: {
      local: {
        get: async () => stored,
        set: async (value) => Object.assign(stored, value),
        remove: async (key) => delete stored[key],
      },
      session: {
        get: async () => session,
        set: async (value) => Object.assign(session, value),
      },
    },
    action: {
      setBadgeText: async () => {},
      setBadgeBackgroundColor: async () => {},
    },
    windows: {
      getAll: async () =>
        [{ id: 1 }, { id: 2 }].filter((window) => !removed.includes(window.id)),
      remove: async (id) => {
        if (options.failRemovalOnce && id === 2 && !failedRemoval) {
          failedRemoval = true;
          throw new Error("fixture denied");
        }
        removed.push(id);
      },
    },
    alarms: { create: async () => {}, onAlarm: event() },
    tabs: {
      query: async () => [
        {
          id: 9,
          url: options.tabUrl ?? "https://billing.test/account",
          incognito: false,
        },
      ],
    },
    scripting: {
      executeScript: async () => {
        scripts++;
        return [
          {
            result: {
              origin: "https://billing.test",
              captured: [{ access_token: "fixture-token" }],
            },
          },
        ];
      },
    },
    permissions: { contains: async () => true },
    proxy: {
      settings: {
        get: async () => ({ value: { mode: "system" } }),
        onChange: {
          addListener: (fn) => {
            events.proxy = fn;
          },
        },
      },
    },
    cookies: {
      getAllCookieStores: async () => [{ id: "profile-store", tabIds: [9] }],
      getAll: async (filter) => {
        assert.equal(filter.storeId, "profile-store");
        return [
          { name: "session", value: "fixture-cookie", domain: "billing.test" },
        ];
      },
    },
  };
  const fetcher = async (url, init) => {
    requests.push({ url, init });
    if (url.endsWith("/v1/arm")) {
      if (options.delayArm)
        await new Promise((resolve) => {
          finishArm = resolve;
        });
      server = status({
        armed: true,
        safe: true,
        ip: "203.0.113.10",
        reason: "allowed",
      });
      sockets.at(-1).emit(server);
      return Response.json(server);
    }
    if (url.endsWith("/v1/disarm")) {
      server = status();
      return Response.json(server);
    }
    if (url.endsWith("/v1/stations"))
      return Response.json([
        { id: "fixture-site", name: "Fixture", origin: "https://billing.test" },
      ]);
    if (url.endsWith("/v1/station/connect"))
      return Response.json({ account: "fixture" });
    if (url.endsWith("/api/pricing")) return Response.json({ data: [] });
    if (url.endsWith("/v1/station/pricing"))
      return Response.json({ models: [] });
    throw new Error("Unexpected fake endpoint");
  };
  vm.runInNewContext(source, {
    ...core,
    captureCurrentAccount,
    chrome,
    WebSocket: Socket,
    fetch: fetcher,
    AbortSignal,
    TextDecoder,
    TextEncoder,
    URL,
    Response,
    Uint8Array,
    navigator: { connection: network },
    addEventListener: (name, fn) => {
      events[name] = fn;
    },
    performance: { now: () => clock },
    setInterval: (callback) => {
      timers.push(callback);
    },
    setTimeout: () => 1,
    clearTimeout: () => {},
  });
  const send = (type, data = {}, sender) =>
    new Promise((resolve) => {
      const accepted = handler(
        { type, ...data },
        sender ?? {
          id: chrome.runtime.id,
          url: chrome.runtime.getURL("popup.html"),
        },
        resolve,
      );
      if (!accepted) resolve({ rejected: true });
    });
  await flush();
  return {
    send,
    removed,
    session,
    requests,
    sockets,
    events,
    network,
    messages,
    authenticate: () => pendingAuth?.(),
    finishArm: () => finishArm?.(),
    scripts: () => scripts,
    advance: async (ms) => {
      clock += ms;
      for (const timer of timers) timer();
      await flush();
    },
  };
}

test("a connection drop while not protected does not close any browser windows", async () => {
  const f = await fixture();
  assert.equal((await f.send("state")).value.connected, true);
  f.sockets.at(-1).close();
  await flush();
  assert.deepEqual(f.removed, []);
});
test("an IP change closes every window in the current profile, even if the new IP is allowed", async () => {
  const f = await fixture();
  assert.equal((await f.send("arm")).ok, true);
  assert.equal(f.session.protection.armed, true);
  f.sockets
    .at(-1)
    .emit(status({ armed: true, safe: true, ip: "203.0.113.11" }));
  await flush();
  assert.deepEqual(f.removed, [1, 2]);
  assert.equal((await f.send("state")).value.armed, false);
});
test("unknown gate and disconnected bridge both close after explicit arm", async () => {
  for (const kind of ["unknown", "disconnect"]) {
    const f = await fixture();
    await f.send("arm");
    if (kind === "unknown")
      f.sockets
        .at(-1)
        .emit(status({ armed: true, safe: false, reason: "unknown IP" }));
    else f.sockets.at(-1).close();
    await flush();
    assert.deepEqual(f.removed, [1, 2]);
  }
});
test("missing heartbeats cannot be masked by a live but silent websocket", async () => {
  const f = await fixture();
  await f.send("arm");
  await f.advance(core.STALE_MS + 1);
  assert.deepEqual(f.removed, [1, 2]);
});
test("worker restoration closes windows before reconnect when previous protection was active", async () => {
  const f = await fixture({ protection: { armed: true } });
  assert.deepEqual(f.removed, [1, 2]);
  assert.equal((await f.send("state")).value.armed, false);
  assert.equal(f.session.protection.armed, false);
});
test("explicit disarm stops future automatic closes", async () => {
  const f = await fixture();
  await f.send("arm");
  assert.equal((await f.send("disarm")).ok, true);
  f.sockets.at(-1).close();
  await flush();
  assert.deepEqual(f.removed, []);
});
test("webpages and unrelated extension contexts cannot command the service worker", async () => {
  const f = await fixture();
  assert.equal(
    (
      await f.send(
        "arm",
        {},
        { id: "a".repeat(32), url: "https://billing.test" },
      )
    ).rejected,
    true,
  );
  assert.deepEqual(f.removed, []);
});
test("a changed tab origin blocks account extraction before scripting executes", async () => {
  const f = await fixture({ tabUrl: "https://attacker.test" });
  const result = await f.send("import", {
    stationId: "fixture-site",
    origin: "https://billing.test",
    backend: "newapi",
  });
  assert.equal(result.ok, false);
  assert.equal(f.scripts(), 0);
});
test("account imports use the active cookie store and authenticated loopback only", async () => {
  const f = await fixture();
  const result = await f.send("import", {
    stationId: "fixture-site",
    origin: "https://billing.test",
    backend: "newapi",
  });
  assert.equal(result.ok, true);
  const request = f.requests.find((item) =>
    item.url.endsWith("/v1/station/connect"),
  );
  assert.equal(request.init.headers.Authorization, `Bearer ${"a".repeat(64)}`);
  assert.equal(JSON.parse(request.init.body).cookie, "session=fixture-cookie");
  assert.equal(JSON.parse(request.init.body).backend, "newapi");
  assert.ok(
    f.requests.every((item) => item.url.startsWith("http://127.0.0.1:27189/")),
  );
});
test("public pricing requests never send a bearer or cookies and never follow redirects", async () => {
  const f = await fixture();
  assert.equal(
    (
      await f.send("pricing", {
        stationId: "fixture-site",
        origin: "https://billing.test",
        backend: "newapi",
      })
    ).ok,
    true,
  );
  const request = f.requests.find(
    (item) => item.url === "https://billing.test/api/pricing",
  );
  assert.equal(request.init.credentials, "omit");
  assert.equal(request.init.redirect, "error");
  assert.equal(request.init.headers.Authorization, undefined);
});

test("stopping protection while connect is pending prevents a delayed arm", async () => {
  const f = await fixture({ delayConnect: true });
  const arming = f.send("arm");
  await flush();
  assert.equal((await f.send("disarm")).ok, true);
  f.authenticate();
  assert.equal((await arming).ok, false);
  assert.equal((await f.send("state")).value.armed, false);
  assert.ok(!f.requests.some((request) => request.url.endsWith("/v1/arm")));
});

test("stopping during a delayed arm response wins and duplicate arm cannot supersede it", async () => {
  const f = await fixture({ delayArm: true });
  const arming = f.send("arm");
  await flush();
  assert.equal((await f.send("arm")).ok, false);
  const stopping = f.send("disarm");
  await flush();
  f.finishArm();
  assert.equal((await arming).ok, false);
  assert.equal((await stopping).ok, true);
  assert.equal((await f.send("state")).value.armed, false);
  assert.equal(f.session.protection.armed, false);
  assert.deepEqual(
    f.requests
      .filter((r) => /\/(?:arm|disarm)$/.test(r.url))
      .map((r) => r.url.split("/").at(-1)),
    ["arm", "disarm"],
  );
});

test("failed window closure stays latched, blocks rearm and retries on the next heartbeat", async () => {
  const f = await fixture({ failRemovalOnce: true });
  await f.send("arm");
  f.sockets
    .at(-1)
    .emit(status({ armed: true, safe: false, reason: "IP unknown" }));
  await flush();
  assert.deepEqual(f.removed, [1]);
  assert.equal((await f.send("state")).value.closing, true);
  assert.equal((await f.send("arm")).ok, false);
  assert.equal((await f.send("disarm")).ok, false);
  await f.advance(5_000);
  assert.deepEqual(f.removed, [1, 2]);
  assert.equal((await f.send("state")).value.closing, false);
  assert.equal(f.session.protection.armed, false);
});

test("Sub2 imports never attach an unrelated New API session cookie", async () => {
  const f = await fixture();
  assert.equal(
    (
      await f.send("import", {
        stationId: "fixture-site",
        origin: "https://billing.test",
        backend: "sub2",
      })
    ).ok,
    true,
  );
  const request = f.requests.find((r) => r.url.endsWith("/v1/station/connect"));
  assert.equal(JSON.parse(request.init.body).cookie, "");
});

test("network quality changes request a recheck without closing; a connection type change closes", async () => {
  const f = await fixture();
  await f.send("arm");
  f.network.rtt = 350;
  f.network.downlink = 0.2;
  f.network.effectiveType = "2g";
  f.events.network();
  await flush();
  assert.deepEqual(f.removed, []);
  assert.ok(f.messages.some((message) => message.type === "recheck"));
  f.network.type = "ethernet";
  f.events.network();
  await flush();
  assert.deepEqual(f.removed, [1, 2]);
});

test("unknown network type still requests a recheck, and offline closes only while protected", async () => {
  const f = await fixture();
  f.events.offline();
  await flush();
  assert.deepEqual(f.removed, []);
  await f.send("arm");
  f.network.type = undefined;
  f.events.network();
  await flush();
  assert.deepEqual(f.removed, []);
  f.events.offline();
  await flush();
  assert.deepEqual(f.removed, [1, 2]);
});

test("duplicate proxy notifications do not close; a changed proxy configuration does", async () => {
  const f = await fixture();
  f.events.proxy({ value: { mode: "direct" } });
  await flush();
  assert.deepEqual(f.removed, []);
  await f.send("arm");
  f.events.proxy({
    value: { mode: "direct" },
    levelOfControl: "controllable_by_this_extension",
  });
  await flush();
  assert.deepEqual(f.removed, []);
  f.events.proxy({
    value: {
      mode: "fixed_servers",
      rules: { singleProxy: { host: "fixture.invalid", port: 9999 } },
    },
  });
  await flush();
  assert.deepEqual(f.removed, [1, 2]);
});
