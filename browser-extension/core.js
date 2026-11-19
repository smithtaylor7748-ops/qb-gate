// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 插件里不碰 chrome.* 的纯逻辑：证明、状态取舍、受保护网站名单、拦截规则、弹窗该说什么。
// 后台（background.js）和各个页面都从这里拿；单测（core.node.mjs）直接测这里。
// 线协议只照 docs/DESIGN-NOTES.zh-CN.md「浏览器助手连接协议（协议 2）」。

export const PROTOCOL = 2;
/** 至少每 15 秒要 ping 一次，桥那边才不断开；这里 5 秒一次。 */
export const PING_MS = 5_000;
/** 握手（连上 → 收到 welcome）最多等多久；端口被别的程序占着、不回话时靠它放弃。 */
export const HANDSHAKE_MS = 8_000;
/** 桥每 2 秒推一次状态；这么久一条消息都没收到，就当连接已经坏了（比如睡醒之后的半开连接），断开重连。 */
export const SILENCE_MS = 15_000;
/** 桥给的 ttl_ms 只信这么多：心跳每 2 秒一次，正常 10 秒。 */
export const MAX_TTL_MS = 60_000;
/** 动态规则（浏览器重启也还在）与会话规则（重启就没了）的编号。 */
export const BLOCK_RULE_ID = 1;
export const ALLOW_RULE_ID = 2;
export const MAX_SITES = 50;

/** 默认受保护网站：Claude 官方网页用到的域名（各自连同子域名）。 */
export const DEFAULT_SITES = Object.freeze([
  "claude.ai",
  "claude.com",
  "anthropic.com",
  "claudeusercontent.com",
]);

/** declarativeNetRequest 的全部请求类型；不写 resourceTypes 时 Chrome 会漏掉主框架。 */
export const RESOURCE_TYPES = Object.freeze([
  "main_frame",
  "sub_frame",
  "stylesheet",
  "script",
  "image",
  "font",
  "object",
  "xmlhttprequest",
  "ping",
  "csp_report",
  "media",
  "websocket",
  "webtransport",
  "webbundle",
  "other",
]);

/** 只有出口真的变了才要使用者点「重新放行」；别的不放行原因插件会带着上次放行的出口自己再核一次。 */
export const NEEDS_USER = Object.freeze(new Set(["ip_changed"]));

const CODES = new Set([
  "not_armed",
  "checking",
  "allowed",
  "ip_changed",
  "ip_unknown",
  "ip_not_allowed",
  "country_not_allowed",
  "country_unknown",
  "country_conflict",
  "probe_timeout",
  "manual_paused",
  "config_changed",
  "stale",
  "disconnected",
  "panel_stopping",
]);

// ------------------------------------------------------------------ 小工具

export const isHex = (value, length) =>
  typeof value === "string" &&
  value.length === length &&
  /^[0-9a-f]+$/.test(value);

export function hex(bytes) {
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}

export function randomHex(bytes = 32) {
  return hex(globalThis.crypto.getRandomValues(new Uint8Array(bytes)));
}

/** 不因为前缀对上几位就早退。 */
export function sameSecret(a, b) {
  if (typeof a !== "string" || typeof b !== "string" || a.length !== b.length)
    return false;
  let different = 0;
  for (let i = 0; i < a.length; i++)
    different |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return different === 0;
}

/** 协议第八节：hex(HMAC-SHA256(口令, "qb-link/2 <角色>\n<a>\n<b>"))。 */
export async function proof(token, role, a, b) {
  const encoder = new TextEncoder();
  const key = await globalThis.crypto.subtle.importKey(
    "raw",
    encoder.encode(token),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = await globalThis.crypto.subtle.sign(
    "HMAC",
    key,
    encoder.encode(`qb-link/${PROTOCOL} ${role}\n${a}\n${b}`),
  );
  return hex(new Uint8Array(signature));
}

/** 桥回的一句话：截短、去掉换行，避免把奇怪的内容整段搬到界面上。 */
export function shortText(value, max = 300) {
  return typeof value === "string"
    ? value
        .replace(/[\r\n\t]+/g, " ")
        .trim()
        .slice(0, max)
    : "";
}

export function validPort(value) {
  const port = typeof value === "string" ? Number(value) : value;
  return Number.isInteger(port) && port > 0 && port < 65536 ? port : null;
}

// ------------------------------------------------------------------ 配对

/** 存在 chrome.storage.local 里的配对：编号、长期口令、端口。形状不对就当没有。 */
export function validPairing(value) {
  return (
    !!value &&
    typeof value === "object" &&
    isHex(value.id, 16) &&
    isHex(value.token, 64) &&
    validPort(value.port) !== null
  );
}

/** 认领页面的地址：http://127.0.0.1:<端口>/link。端口从这里取，不信消息里报的。 */
export function claimPagePort(url) {
  try {
    const page = new URL(url);
    if (
      page.protocol !== "http:" ||
      page.hostname !== "127.0.0.1" ||
      page.pathname !== "/link" ||
      page.username ||
      page.password
    )
      return null;
    return validPort(page.port);
  } catch {
    return null;
  }
}

/** /link/claim 的成功回包（第四、五节）。 */
export function validClaimReply(value) {
  return (
    !!value &&
    typeof value === "object" &&
    (value.type === "paired" || value.type === "relinked") &&
    value.protocol === PROTOCOL &&
    isHex(value.pairing, 16) &&
    validPort(value.port) !== null &&
    isHex(value.proof, 64) &&
    (value.type === "relinked" || isHex(value.token, 64))
  );
}

/**
 * 核对认领回包，算出要存的配对。`existing` 是本地原来的配对（没有就是 null）。
 * 新配对：口令就在回包里（首次信任，见协议第四节），证明只说明回包没被改；
 * 重连：必须是原来那条配对，证明用本地的口令核。
 */
export async function acceptClaim(reply, claim, existing) {
  if (!validClaimReply(reply)) throw new Error("QB Gate 的认领回包格式不对");
  const token = reply.type === "paired" ? reply.token : existing?.token;
  if (reply.type === "relinked" && (!existing || existing.id !== reply.pairing))
    throw new Error("QB Gate 说这是重连，但插件里没有这条配对");
  const expected = await proof(token, "claimed", reply.pairing, claim);
  if (!sameSecret(expected, reply.proof))
    throw new Error("QB Gate 的证明对不上，没有保存这次连接");
  return { id: reply.pairing, token, port: reply.port };
}

// ------------------------------------------------------------------ 保护通道的状态

/** 桥推来的 status（第七节）。 */
export function validStatus(value) {
  return (
    !!value &&
    typeof value === "object" &&
    value.type === "status" &&
    Number.isSafeInteger(value.seq) &&
    value.seq >= 0 &&
    typeof value.allow === "boolean" &&
    CODES.has(value.code) &&
    typeof value.reason === "string" &&
    typeof value.armed === "boolean" &&
    typeof value.checking === "boolean" &&
    (value.ip === null || typeof value.ip === "string") &&
    (value.baseline === null || typeof value.baseline === "string") &&
    Number.isFinite(value.checked_at) &&
    Number.isFinite(value.ttl_ms)
  );
}

/**
 * 每条连接一份：只采用序号不比已有的小的状态（晚到的 HTTP 回包盖不掉新的推送），
 * `allow: true` 只在 ttl_ms 内算数。时间用两个钟一起看：单调钟防改系统时间，
 * 墙上时钟防睡眠期间单调钟不走 —— 任何一个说过期了就算过期。
 */
export class StatusTracker {
  constructor() {
    this.reset();
  }
  reset() {
    this.seq = -1;
    this.status = null;
    this.until = 0;
    this.wallUntil = 0;
    this.freshAfter = 0;
  }
  /** 浏览器的网络 / 代理变了：这之前核过的放行都不算，等桥复查出新结果（checked_at 是秒）。 */
  requireFresh(wall) {
    this.freshAfter = Math.floor(wall / 1000);
  }
  accept(value, now, wall) {
    if (!validStatus(value) || value.seq < this.seq) return false;
    this.seq = value.seq;
    this.status = value;
    if (value.allow) {
      const ttl = Math.max(0, Math.min(value.ttl_ms, MAX_TTL_MS));
      this.until = now + ttl;
      this.wallUntil = wall + ttl;
    } else {
      this.until = 0;
      this.wallUntil = 0;
    }
    return true;
  }
  allowed(now, wall) {
    return (
      !!this.status?.allow &&
      now < this.until &&
      wall < this.wallUntil &&
      this.status.checked_at >= this.freshAfter
    );
  }
  /** 放行还剩多少毫秒（没放行是 0）。 */
  remaining(now, wall) {
    return this.allowed(now, wall)
      ? Math.min(this.until - now, this.wallUntil - wall)
      : 0;
  }
}

/** 没放行时第 n 次自动再核（n 从 1 起）要等多久：15、30、60 秒封顶（协议要求至少 15 秒）。 */
export function armBackoff(attempt) {
  return [15_000, 30_000, 60_000][Math.min(Math.max(attempt, 1), 3) - 1];
}

/** 连不上时第 n 次重连要等多久。 */
export function reconnectBackoff(attempt) {
  return [1_000, 2_000, 5_000, 10_000, 20_000, 30_000][
    Math.min(Math.max(attempt, 1), 6) - 1
  ];
}

/**
 * 连上之后该做什么。
 * - `wait`：放行中、正在核验、或者还没到下一次自动再核的时间；
 * - `user`：出口变了，等使用者点「重新放行」；
 * - `arm`：还没开启，发 arm（带上次放行的出口）；
 * - `rearm`：开过、但这会儿不放行（查不到、过期、手动关门……）：先 disarm 再 arm（带上次放行的出口）。
 *   带着出口去核，只有出口跟使用者放行过的那一个一模一样才会重新放行，不会悄悄换基准。
 */
export function nextStep({ protect, status, busy, now, notBefore }) {
  if (!protect || !status || busy) return { kind: "wait" };
  if (status.allow || status.checking) return { kind: "wait" };
  if (status.armed && NEEDS_USER.has(status.code)) return { kind: "user" };
  if (now < notBefore) return { kind: "wait", at: notBefore };
  return { kind: status.armed ? "rearm" : "arm" };
}

// ------------------------------------------------------------------ 受保护网站

/**
 * 使用者填的一个网站 → 域名（连同子域名一起保护）。认 `claude.ai`、`https://claude.ai/new`、`*.claude.ai`；
 * 不认 IP、本机地址、只有一段的名字 —— 拦了本机就连不上 QB Gate 了。
 */
export function normalizeSite(input) {
  if (typeof input !== "string") return null;
  let text = input.trim().toLowerCase();
  if (!text || text.length > 300) return null;
  text = text.replace(/^\*\./, "").replace(/^\./, "");
  if (!/^[a-z][a-z0-9+.-]*:\/\//.test(text)) text = `https://${text}`;
  let host;
  try {
    const url = new URL(text);
    if (
      !["http:", "https:"].includes(url.protocol) ||
      url.username ||
      url.password
    )
      return null;
    host = url.hostname.replace(/\.$/, "");
  } catch {
    return null;
  }
  if (
    host.length > 253 ||
    !host.includes(".") ||
    host === "localhost" ||
    host.endsWith(".localhost") ||
    /^[\d.]+$/.test(host) ||
    host.startsWith("[")
  )
    return null;
  const labels = host.split(".");
  if (
    labels.some((label) => !/^(?!-)[a-z0-9-]{1,63}(?<!-)$/.test(label)) ||
    /^\d+$/.test(labels[labels.length - 1])
  )
    return null;
  return host;
}

/** 名单去重、去掉被父域名覆盖的子域名、排序。不合格的一条就整份拒绝（说清楚是哪一条）。 */
export function normalizeSites(list) {
  if (!Array.isArray(list)) throw new Error("受保护网站名单格式不对");
  const out = new Set();
  for (const item of list) {
    const site = normalizeSite(item);
    if (!site)
      throw new Error(
        `「${shortText(String(item), 80)}」不是一个能保护的网站域名`,
      );
    out.add(site);
  }
  const sites = [...out].filter(
    (site) =>
      ![...out].some((other) => other !== site && site.endsWith(`.${other}`)),
  );
  if (sites.length > MAX_SITES)
    throw new Error(`受保护网站最多 ${MAX_SITES} 个`);
  return sites.sort();
}

/** chrome.tabs.query / permissions 用的匹配模式：`*://*.claude.ai/*` 同时匹配 claude.ai 和它的子域名。 */
export function sitePatterns(sites) {
  return sites.map((site) => `*://*.${site}/*`);
}

/** 这个网址是不是受保护网站（http / https，本域或子域）。 */
export function protectedSite(url, sites) {
  try {
    const page = new URL(url);
    if (!["http:", "https:"].includes(page.protocol)) return null;
    const host = page.hostname.replace(/\.$/, "");
    return (
      sites.find((site) => host === site || host.endsWith(`.${site}`)) ?? null
    );
  } catch {
    return null;
  }
}

/** 拦截规则（动态规则，浏览器重启也还在）：受保护网站的一切请求。保护关着或名单空就没有。 */
export function blockRules(sites) {
  if (!sites.length) return [];
  return [
    {
      id: BLOCK_RULE_ID,
      priority: 1,
      action: { type: "block" },
      condition: {
        requestDomains: [...sites],
        resourceTypes: [...RESOURCE_TYPES],
      },
    },
  ];
}

/** 放行规则（会话规则，浏览器一重启就没了）：优先级比拦截高。 */
export function allowRules(sites) {
  if (!sites.length) return [];
  return [
    {
      id: ALLOW_RULE_ID,
      priority: 2,
      action: { type: "allow" },
      condition: {
        requestDomains: [...sites],
        resourceTypes: [...RESOURCE_TYPES],
      },
    },
  ];
}

/** 拦截页的地址带着原来要去的网址，放行后能回去。 */
export function blockedPage(base, from) {
  return `${base}?from=${encodeURIComponent(from)}`;
}

/**
 * 拦截页拿到的「原来要去的网址」：只认 http / https（拦截页只由插件自己打开，网页打不开它）。
 * 那个网站后来被移出了名单的话也照样回去 —— 它已经不受保护了。
 */
export function returnTarget(search) {
  try {
    const from = new URLSearchParams(search).get("from");
    if (!from || from.length > 4096) return null;
    const url = new URL(from);
    return ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password
      ? url.href
      : null;
  } catch {
    return null;
  }
}

// ------------------------------------------------------------------ 给人看的话

const OFFLINE = {
  panel_exit: "QB Gate 面板已退出",
  restarting: "QB Gate 正在重启浏览器联动",
};

export function offlineReason(message) {
  return (
    OFFLINE[message?.code] ??
    (shortText(message?.reason) || "QB Gate 断开了连接")
  );
}

/**
 * 弹窗 / 引导页 / 拦截页首屏显示什么：一个状态 + 一个主按钮。
 * `state` 是后台 `publicState()` 给的形状。
 */
export function describe(state) {
  const status = state.status;
  const pause = state.protect ? [{ action: "pause", label: "暂停保护" }] : [];
  if (!state.paired) {
    return {
      tone: state.protect ? "danger" : "muted",
      title: state.protect
        ? "受保护网站已拦截：还没连上 QB Gate"
        : "还没连上 QB Gate",
      detail:
        (state.linkReason ? `${state.linkReason}。` : "") +
        "打开 QB Gate 面板，在「扩展 → 浏览器助手」点「连接浏览器」。浏览器会打开一个确认页，插件自动完成连接，不用复制粘贴。",
      primary: { action: "guide", label: "连接 QB Gate" },
      secondary: pause,
    };
  }
  if (state.link !== "connected") {
    const connecting = state.link === "connecting";
    return {
      tone: state.protect ? "danger" : connecting ? "muted" : "warn",
      title: state.protect
        ? "受保护网站已拦截：连不上 QB Gate"
        : connecting
          ? "正在连接 QB Gate…"
          : "连不上 QB Gate",
      detail:
        (state.linkReason ? `${state.linkReason}。` : "") +
        "QB Gate 面板没在运行时插件连不上；打开面板后会自动连上。面板里提示「一键重连」时点它。",
      primary: connecting ? null : { action: "retry", label: "重试连接" },
      secondary: pause,
    };
  }
  if (!state.protect) {
    return {
      tone: "muted",
      title: "已连接，保护未开启",
      detail:
        "开启后，只有 QB Gate 确认出口 IP 没变时，受保护网站（默认是 Claude 官方网站）才能打开。其他网站不受影响。",
      primary: { action: "enable", label: "开启保护" },
      secondary: [],
    };
  }
  if (state.allowed) {
    return {
      tone: "ok",
      title: "保护中：受保护网站可以打开",
      detail: `出口 IP ${status?.ip ?? "已核实"}。出口一变、QB Gate 断开或查不到，受保护网站会立刻被拦下。`,
      primary: null,
      secondary: pause,
    };
  }
  if (
    state.hold === "recheck" ||
    !status ||
    status.checking ||
    state.verifying
  ) {
    return {
      tone: "warn",
      title: "正在核实出口 IP…",
      detail:
        state.hold === "recheck"
          ? "浏览器的网络或代理刚变过，先拦着，等 QB Gate 复查完出口（通常几秒钟）。"
          : "核实之前受保护网站先拦着，通常几秒钟。",
      primary: null,
      secondary: pause,
    };
  }
  if (state.hold === "expired") {
    return {
      tone: "danger",
      title: "已拦截：QB Gate 有一阵没回话",
      detail:
        "放行只在 QB Gate 一直回话时有效（每 2 秒一次）。面板可能卡住了，或者电脑刚睡醒；它一回话就会自动恢复。",
      primary: { action: "retry", label: "重试连接" },
      secondary: pause,
    };
  }
  if (status.armed && NEEDS_USER.has(status.code)) {
    const change =
      status.baseline && status.ip
        ? `（原来 ${status.baseline}，现在 ${status.ip}）`
        : "";
    return {
      tone: "danger",
      title: "已拦截：出口 IP 变了",
      detail: `${shortText(status.reason) || "出口 IP 已变化"}${change}。确认新的出口没问题，再点「重新放行」。`,
      primary: { action: "reallow", label: "重新放行" },
      secondary: pause,
    };
  }
  const wait =
    state.retryInMs > 0
      ? `插件 ${Math.ceil(state.retryInMs / 1000)} 秒后自动再核一次。`
      : "";
  return {
    tone: "danger",
    title: "已拦截受保护网站",
    detail: `${shortText(status.reason) || "QB Gate 没有放行"}。${wait}只有出口 IP 跟你放行过的一样时才会自动恢复。`,
    primary: { action: "retry", label: "立即重新检查" },
    secondary: pause,
  };
}

// ------------------------------------------------------------------ 中转站与浏览器检查（沿用 0.1.0）

/** Expose settings names only; PAC text, proxy hosts and credentials never leave Chrome. */
export function enhancementSummary(webrtc, proxy) {
  const policies = new Set([
    "default",
    "default_public_and_private_interfaces",
    "default_public_interface_only",
    "disable_non_proxied_udp",
  ]);
  const modes = new Set([
    "direct",
    "auto_detect",
    "pac_script",
    "fixed_servers",
    "system",
  ]);
  const controls = new Set([
    "not_controllable",
    "controlled_by_other_extensions",
    "controllable_by_this_extension",
    "controlled_by_this_extension",
  ]);
  return {
    webrtc_policy: policies.has(webrtc?.value) ? webrtc.value : null,
    proxy_mode: modes.has(proxy?.value?.mode) ? proxy.value.mode : null,
    proxy_control: controls.has(proxy?.levelOfControl)
      ? proxy.levelOfControl
      : null,
  };
}

export function webOrigin(value) {
  try {
    const url = new URL(value);
    if (
      !["https:", "http:"].includes(url.protocol) ||
      url.username ||
      url.password
    )
      return null;
    return url.origin;
  } catch {
    return null;
  }
}

export function sameStation(tabUrl, expectedOrigin) {
  return (
    webOrigin(expectedOrigin) === expectedOrigin &&
    webOrigin(tabUrl) === expectedOrigin
  );
}

/** 浏览器检查页的地址：只认本机回环上的一次性路径。 */
export function validProbeUrl(value) {
  try {
    const url = new URL(value);
    return (
      url.protocol === "http:" &&
      url.hostname === "127.0.0.1" &&
      validPort(url.port) !== null &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash &&
      /^\/[a-f0-9]{32}\/$/.test(url.pathname)
    );
  } catch {
    return false;
  }
}

export function sessionCookie(cookies, origin) {
  const host = new URL(origin).hostname;
  const applicable = cookies.filter(
    (cookie) =>
      cookie.name === "session" &&
      typeof cookie.value === "string" &&
      cookie.value.length <= 16_384 &&
      !/[\r\n;]/.test(cookie.value) &&
      (cookie.domain === host || cookie.domain === `.${host}`),
  );
  // Do not silently select among sessions belonging to different cookie stores/partitions.
  const values = [...new Set(applicable.map((cookie) => cookie.value))];
  if (values.length > 1)
    throw new Error("当前站点存在多个不同会话，请在普通窗口重新登录后重试。");
  return values.length ? `session=${values[0]}` : "";
}
