// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.

export const HEARTBEAT_MS = 5_000;
export const STALE_MS = 12_000;

export function parsePairing(text) {
  let value;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("请粘贴 QB Gate 生成的完整配对信息。");
  }
  if (
    !value ||
    typeof value !== "object" ||
    typeof value.endpoint !== "string" ||
    typeof value.token !== "string" ||
    !/^[a-f0-9]{64}$/.test(value.token)
  ) {
    throw new Error("配对信息格式不正确，请在 QB Gate 重新生成。");
  }
  let url;
  try {
    url = new URL(value.endpoint);
  } catch {
    throw new Error("配对地址不正确。");
  }
  if (
    url.protocol !== "http:" ||
    url.hostname !== "127.0.0.1" ||
    !url.port ||
    url.username ||
    url.password ||
    url.search ||
    url.hash ||
    url.pathname !== "/"
  ) {
    throw new Error("配对地址必须是 QB Gate 的本机回环地址与动态端口。");
  }
  return { endpoint: url.origin, token: value.token };
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

export function validStatus(value) {
  return (
    value &&
    typeof value === "object" &&
    typeof value.running === "boolean" &&
    typeof value.connected === "boolean" &&
    typeof value.armed === "boolean" &&
    typeof value.safe === "boolean" &&
    (value.ip === null || typeof value.ip === "string") &&
    typeof value.reason === "string" &&
    value.reason.length <= 1000 &&
    Number.isFinite(value.checked_at) &&
    value.checked_at >= 0
  );
}

/** Pure protection decisions. Clock is monotonic receipt time, never a remote clock. */
export class Protection {
  constructor() {
    this.armed = false;
    this.baselineIp = null;
    this.lastHeartbeat = 0;
    this.reason = "尚未开启 IP 保护";
  }
  arm(status, now) {
    if (
      !validStatus(status) ||
      !status.running ||
      !status.connected ||
      !status.armed ||
      !status.safe ||
      !status.ip
    )
      throw new Error("IP 锁未放行，无法开启浏览器保护。");
    this.armed = true;
    this.baselineIp = status.ip;
    this.lastHeartbeat = now;
    this.reason = "已开启保护";
  }
  accept(status, now) {
    if (!validStatus(status)) return this.armed ? "QB Gate 状态无效" : null;
    this.lastHeartbeat = now;
    if (!this.armed) return null;
    if (!status.running || !status.connected) return "与 QB Gate 的连接已失效";
    if (!status.armed || !status.safe || !status.ip)
      return status.reason || "IP 锁已停止放行";
    if (status.ip !== this.baselineIp) return "出口 IP 已变化";
    return null;
  }
  stale(now) {
    return this.armed && now - this.lastHeartbeat >= STALE_MS
      ? "QB Gate 心跳超时"
      : null;
  }
  disconnected() {
    return this.armed ? "与 QB Gate 的连接已断开" : null;
  }
  disarm(reason = "IP 保护已停止") {
    this.armed = false;
    this.baselineIp = null;
    this.reason = reason;
  }
}

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
