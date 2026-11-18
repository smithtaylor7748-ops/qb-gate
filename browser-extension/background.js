// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 插件后台（MV3 service worker）。三件事：
//   1. 按协议 2 跟 QB Gate 面板的本机连接桥配对、连保护通道（docs/DESIGN-NOTES.zh-CN.md 那一节）；
//   2. 「只拦受保护的网站」：拦截规则是**动态规则**（浏览器重启也还在），放行规则是**会话规则**
//      （浏览器一重启就没了）。所以浏览器重启、后台被回收、面板没开，受保护网站都先拦着，
//      直到这一次重新核实过出口。拦不拦**不依赖**后台活着：后台死了，规则照样在起作用；
//   3. 沿用 0.1.0 的三个附带功能（浏览器检查、查套路用已登录账户、公开价格），改走 /v2/*。
// 这里的取舍（为什么这样分规则、为什么醒来先撤放行）写在 DESIGN-NOTES「浏览器助手插件」一节。
import {
  ALLOW_RULE_ID,
  DEFAULT_SITES,
  HANDSHAKE_MS,
  PING_MS,
  PROTOCOL,
  StatusTracker,
  acceptClaim,
  allowRules,
  armBackoff,
  blockRules,
  blockedPage,
  claimPagePort,
  enhancementSummary,
  isHex,
  nextStep,
  normalizeSites,
  offlineReason,
  proof,
  protectedSite,
  randomHex,
  reconnectBackoff,
  sameSecret,
  sameStation,
  sessionCookie,
  shortText,
  sitePatterns,
  validPairing,
  validPort,
  validProbeUrl,
  validStatus,
  webOrigin,
} from "./core.js";
import { captureCurrentAccount } from "./capture.js";

const VERSION = chrome.runtime.getManifest().version;
const WAKE_ALARM = "qb-wake";
const BLOCKED_PAGE = chrome.runtime.getURL("blocked.html");
const EXTENSION_ORIGIN = chrome.runtime.getURL("").replace(/\/$/, "");
const COLORS = { ok: "#3b7247", danger: "#ac3934" };
/** 这一次后台的编号：后台被回收、重新醒来就换一个（引导页与自动化验收拿它看后台是不是重启过）。 */
const BOOT = randomHex(8);

class CompanionError extends Error {}
const safeError = (error, fallback = "操作失败，请稍后再试。") =>
  error instanceof CompanionError ? error.message : fallback;
const clocks = () => [performance.now(), Date.now()];

// ---- 落盘的（chrome.storage.local）
let pairing = null; // {id, token, port}
let protect = false; // 使用者点了「开启保护」
let baseline = null; // 上次放行时的出口
let sites = [...DEFAULT_SITES];

// ---- 只在这一次后台里的
let conn = null; // 当前这一条保护通道
let link = "unpaired"; // unpaired | connecting | connected | unavailable
let linkReason = "";
let reconnectAttempt = 0;
let reconnectTimer = null;
const tracker = new StatusTracker();
let armBusy = false;
let armAttempt = 0;
let notBefore = 0;
let armTimer = null;
let expiryTimer = null;
let appliedAllow = false; // 放行规则真的已经装上了
let wasAllowed = false;
let sweptKey = null;
let lastRetry = 0;
let networkType = navigator.connection?.type ?? null;
let proxyFingerprint = null;

const allowedNow = () =>
  protect && conn?.phase === "ready" && tracker.allowed(...clocks());

function setLink(state, reason = "") {
  link = state;
  linkReason = reason;
}

// ------------------------------------------------------------------ 规则：拦与放

/** 规则的要紧部分，用来判断「已经是这样了」，幂等地更新。 */
function ruleKey(rules) {
  return JSON.stringify(
    rules
      .map((rule) => [
        rule.id,
        rule.priority,
        rule.action?.type,
        [...(rule.condition?.requestDomains ?? [])].sort(),
        [...(rule.condition?.resourceTypes ?? [])].sort(),
      ])
      .sort(),
  );
}

async function applyRules() {
  const dnr = chrome.declarativeNetRequest;
  const allow = allowedNow();
  const wantBlock = protect ? blockRules(sites) : [];
  const wantAllow = allow ? allowRules(sites) : [];
  const [dynamic, session] = await Promise.all([
    dnr.getDynamicRules(),
    dnr.getSessionRules(),
  ]);
  // 先撤放行、再动拦截、最后才装放行：任何一步失败，留下的都是「拦着」。
  if (!allow && session.length) {
    appliedAllow = false;
    await dnr.updateSessionRules({ removeRuleIds: session.map((r) => r.id) });
  }
  if (ruleKey(dynamic) !== ruleKey(wantBlock))
    await dnr.updateDynamicRules({
      removeRuleIds: dynamic.map((r) => r.id),
      addRules: wantBlock,
    });
  if (allow) {
    if (ruleKey(session) !== ruleKey(wantAllow))
      await dnr.updateSessionRules({
        removeRuleIds: session.map((r) => r.id),
        addRules: wantAllow,
      });
    appliedAllow = allowedNow();
  } else appliedAllow = false;
}

let rulesChain = Promise.resolve();
/** 规则更新排队、每次按最新的状态重算，晚到的不会盖掉新的。 */
function reconcile() {
  const run = rulesChain.then(applyRules, applyRules);
  rulesChain = run.catch(() => {});
  return run;
}

async function badge() {
  const allowed = allowedNow() && appliedAllow;
  const text = !protect ? "" : allowed ? "放" : "拦";
  const title = !protect
    ? link === "connected"
      ? "QB Gate 浏览器助手：已连接，保护未开启"
      : "QB Gate 浏览器助手：保护未开启"
    : allowed
      ? "QB Gate 浏览器助手：保护中，受保护网站可以打开"
      : "QB Gate 浏览器助手：受保护网站已拦截";
  await Promise.allSettled([
    chrome.action.setBadgeText({ text }),
    chrome.action.setBadgeBackgroundColor({
      color: allowed ? COLORS.ok : COLORS.danger,
    }),
    chrome.action.setTitle({ title }),
  ]);
}

/** 这一次不放行是不是已经有了结论（刚开启 / 刚连上、还在核实的时候不去动已经打开的页面）。 */
function settledBlock() {
  if (link === "unavailable" || link === "unpaired") return true;
  const status = tracker.status;
  if (link !== "connected" || !status || status.checking || armBusy)
    return false;
  if (status.allow) return true; // 过期了
  return status.armed || status.code !== "not_armed";
}

async function sweepTabs() {
  let tabs = [];
  try {
    tabs = await chrome.tabs.query({ url: sitePatterns(sites) });
  } catch {
    return;
  }
  await Promise.allSettled(
    tabs
      .filter((tab) => tab.id !== undefined && protectedSite(tab.url, sites))
      .map((tab) =>
        chrome.tabs.update(tab.id, { url: blockedPage(BLOCKED_PAGE, tab.url) }),
      ),
  );
}

function scheduleExpiry() {
  clearTimeout(expiryTimer);
  expiryTimer = null;
  const left = tracker.remaining(...clocks());
  if (left > 0) expiryTimer = setTimeout(() => void onStateChange(), left + 50);
}

/** 状态一变就走一遍：规则、徽标、要不要把已打开的受保护标签页切到拦截页。 */
async function onStateChange() {
  await reconcile().catch(() => {});
  await badge();
  if (allowedNow()) {
    wasAllowed = true;
    sweptKey = null;
    scheduleExpiry();
    return;
  }
  clearTimeout(expiryTimer);
  if (!protect) {
    wasAllowed = false;
    sweptKey = null;
    return;
  }
  if (!wasAllowed && !settledBlock()) return;
  const key = `${link}|${tracker.status?.code ?? ""}|${tracker.status?.seq ?? ""}`;
  if (key === sweptKey) return;
  sweptKey = key;
  wasAllowed = false;
  await sweepTabs();
}

// ------------------------------------------------------------------ 本机请求

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

/** 带着这条连接的会话口令请求桥。会话失效（面板那边断了）就把连接收掉重连。 */
async function request(path, body, timeout = 25_000, c = conn) {
  if (!c || c.phase !== "ready")
    throw new CompanionError("还没连上 QB Gate，请先打开面板。");
  let response;
  try {
    response = await fetch(`http://127.0.0.1:${c.port}${path}`, {
      method: body === undefined ? "GET" : "POST",
      headers: {
        Authorization: `Bearer ${c.session}`,
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
    throw new CompanionError("无法访问 QB Gate，请检查面板是否在运行。");
  }
  let value;
  try {
    value = await boundedJson(response, 4 * 1024 * 1024);
  } catch {
    throw new CompanionError("QB Gate 返回了无法识别的结果。");
  }
  if (
    response.status === 401 &&
    value?.code === "session_invalid" &&
    c === conn
  )
    closeConnection(c, "QB Gate 那边的连接已失效");
  if (!response.ok)
    throw new CompanionError(
      shortText(value?.error) ||
        `QB Gate 请求失败（HTTP ${response.status}）。`,
    );
  return value;
}

/** 开启 / 暂停这类会回状态的请求：回包按序号并进状态。 */
async function call(c, path, body) {
  const value = await request(path, body, 25_000, c);
  if (c === conn && validStatus(value)) tracker.accept(value, ...clocks());
  return value;
}

/** 长请求（账户导入最长约 105 秒）期间每 20 秒调一次扩展接口，免得后台被当成空闲回收。 */
async function keepAlive(work) {
  const timer = setInterval(
    () => void chrome.runtime.getPlatformInfo().catch(() => {}),
    20_000,
  );
  try {
    return await work;
  } finally {
    clearInterval(timer);
  }
}

// ------------------------------------------------------------------ 保护通道

function closeConnection(c, reason) {
  if (!c) return;
  c.ending ??= reason;
  try {
    c.ws.close();
  } catch {
    /* already closing */
  }
}

/** 主动丢掉当前连接（换配对、忘掉配对）：不排重连。 */
function dropConnection() {
  const c = conn;
  if (!c) return;
  conn = null;
  clearInterval(c.ping);
  clearTimeout(c.timer);
  tracker.reset();
  c.ws.onclose = null;
  c.ws.onmessage = null;
  try {
    c.ws.close();
  } catch {
    /* already closing */
  }
}

function ensureConnected() {
  if (!pairing) {
    if (link !== "unpaired") setLink("unpaired", linkReason);
    return;
  }
  if (conn) return;
  clearTimeout(reconnectTimer);
  reconnectTimer = null;
  openSocket(pairing);
}

function scheduleReconnect(delay) {
  clearTimeout(reconnectTimer);
  reconnectAttempt += 1;
  reconnectTimer = setTimeout(
    () => {
      reconnectTimer = null;
      ensureConnected();
    },
    delay ?? reconnectBackoff(reconnectAttempt),
  );
}

function openSocket(p) {
  let ws;
  try {
    ws = new WebSocket(`ws://127.0.0.1:${p.port}/v2/events`);
  } catch {
    setLink("unavailable", "连不上 QB Gate");
    scheduleReconnect();
    return;
  }
  const c = {
    ws,
    port: p.port,
    pairingId: p.id,
    token: p.token,
    nonce: randomHex(32),
    phase: "hello",
    session: null,
    ending: null,
    code: null,
    queue: Promise.resolve(),
    ping: null,
    timer: null,
  };
  conn = c;
  // 重连期间留着上一次连不上的原因，界面照样说得清为什么没连上。
  setLink("connecting", link === "connected" ? "" : linkReason);
  c.timer = setTimeout(() => {
    if (c.phase !== "ready")
      closeConnection(c, "QB Gate 没有回应（这个端口可能被别的程序占用了）");
  }, HANDSHAKE_MS);
  ws.onopen = () => {
    c.opened = true;
    ws.send(
      JSON.stringify({
        type: "hello",
        protocol: PROTOCOL,
        pairing: c.pairingId,
        nonce: c.nonce,
        version: VERSION,
      }),
    );
  };
  ws.onmessage = (event) => {
    c.queue = c.queue
      .then(() => onSocketMessage(c, event.data))
      .catch(() => {
        closeConnection(c, "处理 QB Gate 的消息时出错");
      });
  };
  ws.onclose = () => void onSocketClosed(c);
  ws.onerror = () => {};
  void onStateChange();
}

function rejected(c, message) {
  c.code = typeof message.code === "string" ? message.code : null;
  const reason =
    message.code === "unknown_pairing"
      ? "QB Gate 不认识这个浏览器了（可能在面板里解除过）。请在面板点「连接浏览器」重新连接"
      : message.code === "protocol_mismatch"
        ? "插件和 QB Gate 的版本对不上，请换用 QB Gate 自带的插件"
        : shortText(message.reason) || "QB Gate 拒绝了连接";
  // unknown_pairing 没有经过证明（桥不认得口令就没法证明）：不凭它删本地配对，只照样拦着。
  closeConnection(c, reason);
}

async function onSocketMessage(c, data) {
  if (c !== conn) return;
  let message;
  try {
    if (typeof data !== "string" || data.length > 16_384) throw new Error();
    message = JSON.parse(data);
    if (!message || typeof message !== "object") throw new Error();
  } catch {
    closeConnection(c, "收到了无法识别的消息");
    return;
  }
  if (message.type === "rejected") return rejected(c, message);
  if (c.phase === "hello") {
    if (
      message.type !== "challenge" ||
      message.protocol !== PROTOCOL ||
      !isHex(message.nonce, 64) ||
      !isHex(message.proof, 64)
    )
      return closeConnection(
        c,
        "这个端口上不是 QB Gate（端口可能被别的程序占用了）",
      );
    const expected = await proof(c.token, "server", c.nonce, message.nonce);
    if (!sameSecret(expected, message.proof))
      return closeConnection(
        c,
        "这个端口上不是认得这个浏览器的 QB Gate（端口可能被别的程序占用了）",
      );
    // 桥先证明了自己认得口令，插件才开口。
    const answer = await proof(c.token, "client", c.nonce, message.nonce);
    if (c !== conn) return;
    c.phase = "auth";
    c.ws.send(JSON.stringify({ type: "authenticate", proof: answer }));
    return;
  }
  if (c.phase === "auth") {
    if (
      message.type !== "welcome" ||
      message.protocol !== PROTOCOL ||
      message.pairing !== c.pairingId ||
      !isHex(message.session, 64)
    )
      return closeConnection(c, "QB Gate 的握手回包不对");
    clearTimeout(c.timer);
    c.session = message.session;
    c.phase = "ready";
    tracker.reset();
    reconnectAttempt = 0;
    armAttempt = 0;
    notBefore = 0;
    setLink("connected");
    c.ping = setInterval(() => {
      if (c === conn && c.ws.readyState === WebSocket.OPEN)
        c.ws.send(JSON.stringify({ type: "ping" }));
    }, PING_MS);
    await onStateChange();
    return;
  }
  switch (message.type) {
    case "status":
      if (tracker.accept(message, ...clocks())) await onStatus();
      return;
    case "offline":
      return closeConnection(c, offlineReason(message));
    case "revoked":
      c.revoked = true;
      return closeConnection(
        c,
        "这个浏览器和 QB Gate 的连接已在面板里解除。要再用，请在面板点「连接浏览器」",
      );
    case "replaced":
      c.replaced = true;
      return closeConnection(c, "同一个配对在别处又连上了 QB Gate");
    default:
      return closeConnection(c, "收到了无法识别的消息");
  }
}

async function onSocketClosed(c) {
  clearInterval(c.ping);
  clearTimeout(c.timer);
  if (c !== conn) return;
  conn = null;
  tracker.reset();
  clearTimeout(armTimer);
  if (c.revoked) {
    await forgetPairing(c.ending);
  } else {
    // 连都没连上（面板没开）时，留着上一次更具体的原因（比如「QB Gate 面板已退出」）。
    const keepReason = !c.opened && !c.ending && linkReason;
    setLink(
      "unavailable",
      keepReason
        ? linkReason
        : (c.ending ??
            (c.phase === "ready"
              ? "和 QB Gate 的连接断开了"
              : "连不上 QB Gate（面板可能没在运行）")),
    );
    scheduleReconnect(
      c.replaced || c.code === "unknown_pairing" ? 60_000 : undefined,
    );
  }
  await onStateChange();
}

async function onStatus() {
  const status = tracker.status;
  if (status.allow) {
    armAttempt = 0;
    notBefore = 0;
    const exit = status.baseline || status.ip;
    if (protect && exit && exit !== baseline) {
      baseline = exit;
      await chrome.storage.local.set({ baseline });
    }
  }
  await onStateChange();
  drive();
}

/** 连上之后推进保护：开启、退避后再核、或者等使用者。 */
function drive() {
  clearTimeout(armTimer);
  armTimer = null;
  if (conn?.phase !== "ready") return;
  const step = nextStep({
    protect,
    status: tracker.status,
    busy: armBusy,
    now: Date.now(),
    notBefore,
  });
  if (step.kind === "wait" && step.at)
    armTimer = setTimeout(drive, Math.max(0, step.at - Date.now()));
  else if (step.kind === "arm" || step.kind === "rearm")
    void armNow(step.kind === "rearm", baseline);
}

async function armNow(disarmFirst, base) {
  const c = conn;
  if (!c || armBusy) return;
  armBusy = true;
  void onStateChange();
  try {
    if (disarmFirst) await call(c, "/v2/disarm", {});
    if (protect && c === conn)
      await call(c, "/v2/arm", base ? { baseline: base } : {});
  } catch {
    /* 结果按状态看；连接坏了由 onSocketClosed 处理 */
  } finally {
    armBusy = false;
    if (c === conn) {
      if (protect && !tracker.status?.allow) {
        armAttempt += 1;
        notBefore = Date.now() + armBackoff(armAttempt);
      }
      if (tracker.status?.allow) await onStatus();
      else {
        await onStateChange();
        drive();
      }
    }
  }
}

/** 浏览器的网络或代理变了：先不信旧的放行，请桥立刻复查。 */
function networkChanged(strict) {
  if (strict) {
    tracker.requireFresh(Date.now());
    void onStateChange();
  }
  if (conn?.phase === "ready" && conn.ws.readyState === WebSocket.OPEN)
    conn.ws.send(JSON.stringify({ type: "recheck" }));
  else if (pairing && !reconnectTimer) ensureConnected();
}

// ------------------------------------------------------------------ 配对（认领页面上）

async function claimOnce(port, code, existing) {
  const body = { claim: code, extension_version: VERSION };
  if (existing) {
    body.pairing = existing.id;
    body.proof = await proof(existing.token, "claim", existing.id, code);
  }
  let response;
  try {
    response = await fetch(`http://127.0.0.1:${port}/link/claim`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      credentials: "omit",
      redirect: "error",
      cache: "no-store",
      signal: AbortSignal.timeout(15_000),
    });
  } catch {
    throw new CompanionError("连不上这个页面背后的 QB Gate，请刷新页面再试");
  }
  let value;
  try {
    value = await boundedJson(response, 16 * 1024);
  } catch {
    throw new CompanionError("QB Gate 的认领回包无法识别");
  }
  return { response, value };
}

let claimChain = Promise.resolve();
function claim(code, port) {
  const run = claimChain.then(() => claimNow(code, port));
  claimChain = run.catch(() => {});
  return run;
}

async function claimNow(code, port) {
  let existing = pairing;
  let { response, value } = await claimOnce(port, code, existing);
  if (
    existing &&
    (value?.code === "unknown_pairing" || value?.code === "bad_proof")
  ) {
    // 面板里解除过 / 口令对不上：认领码没有用掉，当新浏览器用同一个码再认领一次（协议第五节）。
    existing = null;
    ({ response, value } = await claimOnce(port, code, null));
  }
  if (!response.ok)
    throw new CompanionError(
      shortText(value?.error) || `认领失败（HTTP ${response.status}）`,
    );
  let next;
  try {
    next = await acceptClaim(value, code, existing);
  } catch (error) {
    throw new CompanionError(error.message);
  }
  if (next.port !== port)
    throw new CompanionError("QB Gate 报的端口跟这个页面对不上，没有保存");
  const changed =
    !pairing ||
    pairing.id !== next.id ||
    pairing.token !== next.token ||
    pairing.port !== next.port;
  pairing = next;
  await chrome.storage.local.set({ pairing });
  if (changed || conn?.phase !== "ready") {
    dropConnection();
    reconnectAttempt = 0;
    ensureConnected();
  }
  await onStateChange();
  return { type: value.type };
}

async function forgetPairing(reason = "") {
  dropConnection();
  clearTimeout(reconnectTimer);
  reconnectTimer = null;
  pairing = null;
  await chrome.storage.local.remove("pairing");
  setLink("unpaired", reason);
  await onStateChange();
}

// ------------------------------------------------------------------ 使用者的操作

async function enable() {
  protect = true;
  baseline = null; // 开启 = 认可这一次 QB Gate 放行的出口
  armAttempt = 0;
  notBefore = 0;
  await chrome.storage.local.set({ protect: true, baseline: null });
  await onStateChange();
  // 桥那边可能还留着上一次开过的状态（比如暂停时没连上）：开过就先暂停再开。
  if (conn?.phase === "ready")
    await armNow(Boolean(tracker.status?.armed), null);
  else ensureConnected();
}

async function pause() {
  protect = false;
  baseline = null;
  await chrome.storage.local.set({ protect: false, baseline: null });
  await onStateChange(); // 先撤掉全部规则
  if (conn?.phase === "ready")
    await call(conn, "/v2/disarm", {}).catch(() => {});
}

async function reallow() {
  if (!protect) throw new CompanionError("保护没开着。");
  if (conn?.phase !== "ready")
    throw new CompanionError("还没连上 QB Gate，请先打开面板。");
  if (armBusy) throw new CompanionError("正在核实出口，请稍候。");
  armAttempt = 0;
  notBefore = 0;
  // 重新放行 = 先暂停再开启、不带上次的出口：认可现在这个出口（仍要 QB Gate 判定放行）。
  await armNow(true, null);
}

async function retry() {
  const now = Date.now();
  if (now - lastRetry < 3_000) return;
  lastRetry = now;
  if (conn?.phase !== "ready") {
    if (conn?.phase) return; // 正在握手
    reconnectAttempt = 0;
    clearTimeout(reconnectTimer);
    reconnectTimer = null;
    ensureConnected();
    return;
  }
  notBefore = 0;
  drive();
}

async function setSites(list) {
  let next;
  try {
    next = normalizeSites(list);
  } catch (error) {
    throw new CompanionError(error.message);
  }
  if (!next.length) throw new CompanionError("至少要保留一个受保护网站。");
  sites = next;
  await chrome.storage.local.set({ sites });
  sweptKey = null;
  await onStateChange();
  return sites;
}

function publicState() {
  const status = tracker.status;
  const allowed = allowedNow() && appliedAllow;
  return {
    version: VERSION,
    extensionId: chrome.runtime.id,
    boot: BOOT,
    paired: Boolean(pairing),
    pairingId: pairing?.id ?? null,
    port: pairing?.port ?? null,
    link,
    linkReason,
    protect,
    allowed,
    verifying:
      protect &&
      link === "connected" &&
      (armBusy || !status || (!status.armed && status.code === "not_armed")),
    status: status
      ? {
          code: status.code,
          reason: status.reason,
          ip: status.ip,
          baseline: status.baseline,
          armed: status.armed,
          checking: status.checking,
          allow: status.allow,
        }
      : null,
    retryInMs: allowed ? 0 : Math.max(0, notBefore - Date.now()),
    baseline,
    sites: [...sites],
  };
}

// ------------------------------------------------------------------ 浏览器检查、查套路、公开价格（沿用 0.1.0）

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
  const stations = await request("/v2/stations");
  const station = Array.isArray(stations)
    ? stations.find(
        (item) => item.id === stationId && item.origin === expectedOrigin,
      )
    : null;
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
  return keepAlive(
    request(
      "/v2/station/connect",
      {
        station_id: stationId,
        origin,
        backend,
        captured: capture.captured,
        cookie,
      },
      115_000,
    ),
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
  const payload = { station_id: stationId, origin, backend, pricing };
  if (
    new TextEncoder().encode(JSON.stringify(payload)).byteLength >
    2 * 1024 * 1024
  )
    throw new CompanionError("公开价格数据过大，未交回 QB Gate。");
  return request("/v2/station/pricing", payload);
}

async function probe() {
  const [webrtc, proxy] = await Promise.allSettled([
    chrome.privacy.network.webRTCIPHandlingPolicy.get({}),
    chrome.proxy.settings.get({ incognito: false }),
  ]);
  await request(
    "/v2/probe/enhancement",
    enhancementSummary(
      webrtc.status === "fulfilled" ? webrtc.value : null,
      proxy.status === "fulfilled" ? proxy.value : null,
    ),
  );
  const result = await request("/v2/probe/start", {});
  if (!validProbeUrl(result?.url))
    throw new CompanionError("检测地址无效，请重新连接 QB Gate。");
  await chrome.tabs.create({ url: new URL(result.url).href });
  return { message: "已在这个浏览器打开检测页，结果会交回 QB Gate。" };
}

// ------------------------------------------------------------------ 启动

const ready = (async () => {
  // 后台是刚醒的，这一次还没有核实过出口：先撤掉放行（被强行结束之后可能残留着）。
  await chrome.declarativeNetRequest
    .updateSessionRules({ removeRuleIds: [ALLOW_RULE_ID] })
    .catch(() => {});
  const local = await chrome.storage.local.get([
    "pairing",
    "protect",
    "baseline",
    "sites",
  ]);
  pairing = validPairing(local.pairing) ? local.pairing : null;
  protect = local.protect === true;
  baseline = typeof local.baseline === "string" ? local.baseline : null;
  try {
    sites = Array.isArray(local.sites)
      ? normalizeSites(local.sites)
      : [...DEFAULT_SITES];
    if (!sites.length) sites = [...DEFAULT_SITES];
  } catch {
    sites = [...DEFAULT_SITES];
  }
  const proxy = await chrome.proxy.settings
    .get({ incognito: false })
    .catch(() => null);
  // 只拿来比「变没变」，留在后台内存里，不交给面板、不写日志。
  proxyFingerprint = proxy?.value ? JSON.stringify(proxy.value) : null;
  if (!(await chrome.alarms.get(WAKE_ALARM)))
    await chrome.alarms.create(WAKE_ALARM, { periodInMinutes: 0.5 });
  if (!pairing) setLink("unpaired");
  await onStateChange();
  ensureConnected();
})();

chrome.runtime.onInstalled.addListener((details) => {
  void ready.then(async () => {
    if (details.reason === "install") {
      await chrome.storage.local.set({ sites });
      await chrome.tabs.create({ url: chrome.runtime.getURL("welcome.html") });
    }
  });
});

// 后台被当成空闲回收之前（能收到的话）先撤放行；收不到时下一次醒来第一件事也是撤放行。
chrome.runtime.onSuspend?.addListener(() => {
  void chrome.declarativeNetRequest.updateSessionRules({
    removeRuleIds: [ALLOW_RULE_ID],
  });
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name !== WAKE_ALARM) return;
  void ready.then(async () => {
    if (!conn && !reconnectTimer) ensureConnected();
    await onStateChange();
  });
});

// 新打开（或从前进后退缓存恢复）的受保护网站：没放行就切到拦截页，说清楚原因。
chrome.tabs.onUpdated.addListener((tabId, info, tab) => {
  const url = info.url ?? (info.status === "loading" ? tab.url : undefined);
  if (!url || url.startsWith(EXTENSION_ORIGIN)) return;
  void ready.then(async () => {
    if (!protect || (allowedNow() && appliedAllow)) return;
    if (!protectedSite(url, sites)) return;
    await chrome.tabs
      .update(tabId, { url: blockedPage(BLOCKED_PAGE, url) })
      .catch(() => {});
  });
});

globalThis.addEventListener?.(
  "online",
  () => void ready.then(() => networkChanged(true)),
);
globalThis.addEventListener?.(
  "offline",
  () => void ready.then(() => networkChanged(true)),
);
navigator.connection?.addEventListener?.("change", () => {
  const current = navigator.connection?.type ?? null;
  const changed = Boolean(networkType && current && networkType !== current);
  networkType = current;
  void ready.then(() => networkChanged(changed));
});
chrome.proxy.settings.onChange.addListener((details) => {
  const next = details?.value ? JSON.stringify(details.value) : null;
  const changed = next !== proxyFingerprint;
  proxyFingerprint = next;
  if (changed) void ready.then(() => networkChanged(true));
});

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (sender.id !== chrome.runtime.id) return false;
  const fromPage =
    typeof sender.url === "string" &&
    sender.url.startsWith(`${EXTENSION_ORIGIN}/`);
  const claimPort =
    !fromPage && sender.tab && sender.frameId === 0
      ? claimPagePort(sender.url)
      : null;
  if (!fromPage && !(message?.type === "claim" && claimPort)) return false;
  void (async () => {
    await ready;
    if (!fromPage) {
      if (!isHex(message.claim, 64)) throw new CompanionError("认领码格式不对");
      return claim(message.claim, claimPort);
    }
    switch (message?.type) {
      case "state":
        return publicState();
      case "guide":
        await chrome.tabs.create({
          url: chrome.runtime.getURL("welcome.html"),
        });
        return publicState();
      case "options":
        await chrome.runtime.openOptionsPage();
        return publicState();
      case "enable":
        await enable();
        return publicState();
      case "pause":
        await pause();
        return publicState();
      case "reallow":
        await reallow();
        return publicState();
      case "retry":
        await retry();
        return publicState();
      case "sites":
        return setSites(message.sites);
      case "forget":
        await forgetPairing("");
        return publicState();
      case "stations":
        return request("/v2/stations");
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
