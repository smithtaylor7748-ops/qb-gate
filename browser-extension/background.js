// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
import {
  HEARTBEAT_MS,
  Protection,
  enhancementSummary,
  parsePairing,
  sameStation,
  sessionCookie,
  validStatus,
  webOrigin,
} from "./core.js";
import { captureCurrentAccount } from "./capture.js";

const protection = new Protection();
let pairing = null;
let socket = null;
let connection = null;
let latest = null;
let connected = false;
let armPending = false;
let tripPromise = null;
let closingReason = null;
let actionEpoch = 0;
let armAttempt = null;
let serverControl = Promise.resolve();
let connectionVersion = 0;
let networkType = navigator.connection?.type ?? null;
let proxyConfiguration = null;
const now = () => performance.now();

async function badge() {
  await chrome.action.setBadgeText({
    text: protection.armed ? "锁" : connected ? "连" : "",
  });
  await chrome.action.setBadgeBackgroundColor({
    color: protection.armed ? "#147d52" : "#516177",
  });
}

async function trip(reason) {
  if (!protection.armed && !armPending && !closingReason) return;
  if (tripPromise) return tripPromise;
  closingReason = reason;
  actionEpoch++;
  armAttempt = null;
  protection.disarm(reason);
  armPending = false;
  const stop = connected
    ? control("/v1/disarm").catch(() => {})
    : Promise.resolve();
  tripPromise = (async () => {
    // A worker restart while closing must finish the close operation.
    await chrome.storage.session
      .set({
        protection: { armed: true, closing: true, reason },
      })
      .catch(() => {});
    let failed = true;
    try {
      const windows = await chrome.windows.getAll({ populate: false });
      await Promise.allSettled(
        windows.map((window) => chrome.windows.remove(window.id)),
      );
      // A rejection may mean a window was already closed; confirm what remains.
      failed = (await chrome.windows.getAll({ populate: false })).length > 0;
    } catch {
      /* Keep the close latched; the next heartbeat retries it. */
    }
    protection.reason = failed ? `${reason}；正在重试关闭剩余窗口` : reason;
    await chrome.storage.session
      .set({
        protection: {
          armed: failed,
          closing: failed,
          reason: protection.reason,
        },
      })
      .catch(() => {});
    await stop;
    if (!failed) closingReason = null;
    await badge().catch(() => {});
  })().finally(() => {
    tripPromise = null;
  });
  return tripPromise;
}

function safeError(error, fallback = "操作失败，请检查 QB Gate 与当前站点。") {
  // All internal errors are our own short messages. Network/Chrome exception strings may include URLs.
  return error instanceof CompanionError ? error.message : fallback;
}
class CompanionError extends Error {}

function control(path, epoch) {
  const result = serverControl
    .catch(() => {})
    .then(() => {
      if (epoch !== undefined && epoch !== actionEpoch)
        throw new CompanionError("开启保护已取消。");
      return request(path, {});
    });
  serverControl = result.catch(() => {});
  return result;
}

async function request(path, body, timeout = 25_000) {
  if (!pairing || !connected) throw new CompanionError("请先连接 QB Gate。");
  let response;
  try {
    response = await fetch(`${pairing.endpoint}${path}`, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        Authorization: `Bearer ${pairing.token}`,
        "X-QB-Extension": chrome.runtime.id,
        "Content-Type": "application/json",
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: "omit",
      redirect: "error",
      cache: "no-store",
      signal: AbortSignal.timeout(timeout),
    });
  } catch {
    throw new CompanionError("无法访问 QB Gate，请检查连接后重试。");
  }
  let value;
  try {
    value = await boundedJson(response, 4 * 1024 * 1024);
  } catch {
    throw new CompanionError("QB Gate 返回了无法识别的结果。");
  }
  if (!response.ok) {
    throw new CompanionError(
      typeof value.error === "string" && value.error.length < 400
        ? value.error
        : `QB Gate 请求失败（HTTP ${response.status}）。`,
    );
  }
  return value;
}

async function boundedJson(response, limit) {
  if (!response.body) throw new Error("empty body");
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw new Error("body too large");
      chunks.push(value);
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  const data = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return JSON.parse(new TextDecoder().decode(data));
}

function disconnectSocket() {
  connectionVersion++;
  connected = false;
  connection = null;
  latest = null;
  const previous = socket;
  socket = null;
  if (previous) previous.close();
}

async function connect() {
  if (connected) return;
  if (connection) return connection;
  if (!pairing) throw new CompanionError("请先粘贴配对信息。");
  const version = ++connectionVersion;
  connection = new Promise((resolve, reject) => {
    const ws = new WebSocket(
      `${pairing.endpoint.replace("http:", "ws:")}/v1/events`,
    );
    socket = ws;
    let finished = false;
    const timer = setTimeout(() => {
      ws.close();
      fail();
    }, 8_000);
    const fail = () => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      reject(
        new CompanionError(
          "连接失败，请确认 QB Gate 的浏览器联动已开启并重新配对。",
        ),
      );
    };
    ws.onopen = () =>
      ws.send(JSON.stringify({ type: "authenticate", token: pairing.token }));
    ws.onmessage = (event) => {
      if (version !== connectionVersion) return;
      let value;
      try {
        if (typeof event.data !== "string" || event.data.length > 65_536)
          throw new Error();
        value = JSON.parse(event.data);
        if (value.type !== "status" || !validStatus(value)) throw new Error();
      } catch {
        void trip("QB Gate 返回了无效状态");
        ws.close();
        fail();
        return;
      }
      connected = true;
      latest = value;
      const reason = protection.accept(value, now());
      if (reason) void trip(reason);
      if (!finished) {
        finished = true;
        clearTimeout(timer);
        resolve();
      }
      void badge();
    };
    ws.onclose = () => {
      if (version !== connectionVersion) return;
      connected = false;
      connection = null;
      if (protection.armed || armPending) void trip("与 QB Gate 的连接已断开");
      fail();
      void badge();
    };
    ws.onerror = () => {
      ws.close();
      fail();
    };
  });
  try {
    await connection;
  } catch (error) {
    connection = null;
    throw error;
  }
}

async function disarm() {
  if (closingReason)
    throw new CompanionError(
      "保护已触发，正在关闭剩余窗口；完成前不能取消关闭。",
    );
  actionEpoch++;
  armAttempt = null;
  armPending = false;
  protection.disarm();
  await chrome.storage.session.set({
    protection: { armed: false, reason: protection.reason },
  });
  await badge();
  if (connected) await control("/v1/disarm");
}

async function arm() {
  if (closingReason || tripPromise)
    throw new CompanionError("窗口关闭尚未完成，请稍后重新开启保护。");
  if (armAttempt !== null || protection.armed)
    throw new CompanionError("浏览器保护已开启或正在核验。");
  const epoch = ++actionEpoch;
  armAttempt = epoch;
  try {
    await connect();
    if (epoch !== actionEpoch) throw new CompanionError("开启保护已取消。");
    armPending = true;
    // Persist before the asynchronous gate check; a suspended/restarted worker must not bypass it.
    await chrome.storage.session.set({
      protection: { armed: true, reason: "正在核验 IP 锁" },
    });
    if (epoch !== actionEpoch) throw new CompanionError("开启保护已取消。");
    const state = await control("/v1/arm", epoch);
    if (epoch !== actionEpoch || !armPending || !connected)
      throw new CompanionError("连接已失效，未开启保护。");
    try {
      protection.arm(state, now());
    } catch {
      throw new CompanionError("IP 锁未放行，无法开启浏览器保护。");
    }
    if (latest?.armed && !latest.safe) {
      await trip(latest.reason || "IP 锁已停止放行");
      throw new CompanionError("IP 锁状态已变化，浏览器保护已触发。");
    }
    armPending = false;
    await chrome.storage.session.set({
      protection: { armed: true, reason: "已开启保护" },
    });
    await badge();
  } catch (error) {
    if (epoch === actionEpoch && armPending) {
      armPending = false;
      protection.disarm("未能开启 IP 保护");
      await chrome.storage.session.set({
        protection: { armed: false, reason: protection.reason },
      });
      if (connected) await control("/v1/disarm").catch(() => {});
    }
    throw error;
  } finally {
    if (armAttempt === epoch) armAttempt = null;
  }
}

async function activeTab(expectedOrigin) {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id || !sameStation(tab.url, expectedOrigin)) {
    throw new CompanionError(
      "当前页面与所选中转站地址不同，请打开对应站点后重试。",
    );
  }
  if (tab.incognito)
    throw new CompanionError("账户读取请使用普通窗口，避免混用隐身会话。");
  return tab;
}

async function stationTarget(stationId, expectedOrigin) {
  const stations = await request("/v1/stations");
  const station = stations.find(
    (item) => item.id === stationId && item.origin === expectedOrigin,
  );
  if (!station || !webOrigin(station.origin))
    throw new CompanionError("站点已更改，请刷新站点列表。");
  return station;
}

async function importAccount({ stationId, origin, backend }) {
  if (!["newapi", "sub2"].includes(backend))
    throw new CompanionError("请选择站点类型。");
  await stationTarget(stationId, origin);
  const tab = await activeTab(origin);
  const [result] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: captureCurrentAccount,
    args: [origin],
    world: "ISOLATED",
  });
  const capture = result?.result;
  if (!capture || capture.origin !== origin || !Array.isArray(capture.captured))
    throw new CompanionError("无法读取当前站点，请刷新页面并完成登录。");
  await activeTab(origin);
  let cookie = "";
  if (
    backend === "newapi" &&
    (await chrome.permissions.contains({ origins: [`${origin}/*`] }))
  ) {
    const stores = await chrome.cookies.getAllCookieStores();
    const store = stores.find((item) => item.tabIds.includes(tab.id));
    if (!store)
      throw new CompanionError("无法确认当前浏览器会话，请重新打开站点。");
    const cookies = await chrome.cookies.getAll({
      url: `${origin}/api/user/self`,
      name: "session",
      storeId: store.id,
    });
    try {
      cookie = sessionCookie(cookies, origin);
    } catch (error) {
      throw new CompanionError(error.message);
    }
  }
  if (!capture.captured.length && !cookie)
    throw new CompanionError("未找到可用的登录会话，请先在当前站点登录。");
  return request(
    "/v1/station/connect",
    {
      station_id: stationId,
      origin,
      backend,
      captured: capture.captured,
      cookie,
    },
    115_000,
  );
}

async function publicPricing({ stationId, origin, backend }) {
  await stationTarget(stationId, origin);
  await activeTab(origin);
  if (!["newapi", "sub2"].includes(backend))
    throw new CompanionError("请选择站点类型。");
  if (!(await chrome.permissions.contains({ origins: [`${origin}/*`] })))
    throw new CompanionError("读取公开价格需要允许访问当前站点。");
  const path = backend === "newapi" ? "/api/pricing" : "/api/v1/model-plaza";
  let response;
  try {
    response = await fetch(`${origin}${path}`, {
      credentials: "omit",
      redirect: "error",
      cache: "no-store",
      signal: AbortSignal.timeout(20_000),
      headers: { Accept: "application/json" },
    });
  } catch {
    throw new CompanionError("无法读取公开价格，请检查站点网络或访问限制。");
  }
  if (!response.ok)
    throw new CompanionError(
      `站点没有提供可访问的公开价格（HTTP ${response.status}）。`,
    );
  let pricing;
  try {
    // Leave room for station identity and JSON escaping under the bridge's 2 MiB cap.
    pricing = await boundedJson(response, 1_800_000);
  } catch {
    throw new CompanionError("公开价格格式无法识别或数据过大。");
  }
  const payload = {
    station_id: stationId,
    origin,
    backend,
    pricing,
  };
  if (
    new TextEncoder().encode(JSON.stringify(payload)).byteLength >
    2 * 1024 * 1024
  ) {
    throw new CompanionError("公开价格数据过大，未交回 QB Gate。");
  }
  return request("/v1/station/pricing", payload);
}

async function probe() {
  const [webrtc, proxy] = await Promise.allSettled([
    chrome.privacy.network.webRTCIPHandlingPolicy.get({}),
    chrome.proxy.settings.get({ incognito: false }),
  ]);
  await request(
    "/v1/probe/enhancement",
    enhancementSummary(
      webrtc.status === "fulfilled" ? webrtc.value : null,
      proxy.status === "fulfilled" ? proxy.value : null,
    ),
  );
  const result = await request("/v1/probe/start", {});
  const url = new URL(result.url);
  if (
    url.protocol !== "http:" ||
    url.hostname !== "127.0.0.1" ||
    !url.port ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    !/^\/[a-f0-9]{32}\/$/.test(url.pathname)
  ) {
    throw new CompanionError("检测地址无效，请重新连接 QB Gate。");
  }
  await chrome.tabs.create({ url: url.href });
  return { message: "已在 Chrome 打开检测页，结果将交回 QB Gate。" };
}

function publicState() {
  return {
    paired: Boolean(pairing),
    connected,
    armed: protection.armed,
    armPending: armPending || armAttempt !== null,
    closing: Boolean(closingReason),
    ip: protection.armed ? protection.baselineIp : (latest?.ip ?? null),
    reason: protection.armed
      ? "保护中：出口变化或连接失效将关闭窗口"
      : protection.reason,
  };
}

const ready = (async () => {
  const [local, session, proxy] = await Promise.all([
    chrome.storage.local.get("pairing"),
    chrome.storage.session.get("protection"),
    chrome.proxy.settings.get({ incognito: false }).catch(() => null),
  ]);
  // Equality only, retained in worker memory; never sent to the panel or logged.
  if (proxy?.value) proxyConfiguration = JSON.stringify(proxy.value);
  if (session.protection?.armed) {
    protection.armed = true;
    await trip("浏览器保护后台曾中断，已关闭窗口；请重新开启保护");
  } else if (typeof session.protection?.reason === "string")
    protection.reason = session.protection.reason;
  if (local.pairing) {
    try {
      pairing = parsePairing(JSON.stringify(local.pairing));
    } catch {
      await chrome.storage.local.remove("pairing");
    }
  }
  await chrome.alarms.create("qb-companion-reconnect", { periodInMinutes: 1 });
  if (pairing) void connect().catch(() => {});
})();

setInterval(() => {
  if (closingReason) void trip(closingReason);
  const reason = protection.stale(now());
  if (reason) void trip(reason);
  if (connected && socket?.readyState === WebSocket.OPEN)
    socket.send(JSON.stringify({ type: "ping" }));
}, HEARTBEAT_MS);

chrome.alarms.onAlarm.addListener(() => {
  void ready.then(async () => {
    if (closingReason) await trip(closingReason);
    const reason = protection.stale(now());
    if (reason) await trip(reason);
    if (pairing && !connected && !protection.armed)
      await connect().catch(() => {});
  });
});
globalThis.addEventListener?.("offline", () => {
  void trip("Chrome 已断网");
});
navigator.connection?.addEventListener("change", () => {
  const current = navigator.connection?.type ?? null;
  const changed = networkType && current && networkType !== current;
  networkType = current;
  if (changed && (protection.armed || armPending))
    void trip("Chrome 网络连接类型已变化");
  if (connected && socket?.readyState === WebSocket.OPEN)
    socket.send(JSON.stringify({ type: "recheck" }));
});
chrome.proxy.settings.onChange.addListener((details) => {
  const next = details?.value ? JSON.stringify(details.value) : null;
  const changed = next !== proxyConfiguration;
  proxyConfiguration = next;
  if (changed && (protection.armed || armPending))
    void trip("Chrome 代理设置已变化");
});

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (
    sender.id !== chrome.runtime.id ||
    sender.url !== chrome.runtime.getURL("popup.html")
  )
    return false;
  void (async () => {
    await ready;
    switch (message?.type) {
      case "state":
        return publicState();
      case "pair": {
        if (
          protection.armed ||
          armPending ||
          armAttempt !== null ||
          closingReason
        )
          throw new CompanionError("请先停止保护，再更换配对。");
        let value;
        try {
          value = parsePairing(message.pairing);
        } catch (error) {
          throw new CompanionError(error.message);
        }
        disconnectSocket();
        pairing = value;
        await chrome.storage.local.set({ pairing });
        await connect();
        return publicState();
      }
      case "connect":
        await connect();
        return publicState();
      case "forget":
        if (closingReason)
          throw new CompanionError("正在关闭剩余窗口，请等待完成后解除配对。");
        await disarm().catch(() => {});
        disconnectSocket();
        pairing = null;
        await chrome.storage.local.remove("pairing");
        await badge();
        return publicState();
      case "arm":
        await arm();
        return publicState();
      case "disarm":
        await disarm();
        return publicState();
      case "stations":
        return request("/v1/stations");
      case "import":
        return importAccount(message);
      case "pricing":
        return publicPricing(message);
      case "probe":
        return probe();
      default:
        throw new CompanionError("未知操作。");
    }
  })().then(
    (value) => respond({ ok: true, value }),
    (error) => respond({ ok: false, error: safeError(error) }),
  );
  return true;
});
