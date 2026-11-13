// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.

/** Serialized by chrome.scripting: keep this function entirely self-contained. */
export function captureCurrentAccount(expectedOrigin) {
  if (location.origin !== expectedOrigin)
    throw new Error("站点地址已变化，请重新选择。");
  const captured = [];
  const tokens = new Set(["auth_token", "access_token", "token"]);
  const userFields = ["id", "username", "email"];
  const text = (value) =>
    typeof value === "string" && value.length <= 16_384 && !/[\r\n]/.test(value)
      ? value
      : null;
  const sanitize = (value, depth = 0) => {
    if (
      !value ||
      typeof value !== "object" ||
      Array.isArray(value) ||
      depth > 3
    )
      return null;
    const result = {};
    for (const key of ["access_token", "token", "user_id", ...userFields]) {
      if (
        typeof value[key] === "number" &&
        Number.isSafeInteger(value[key]) &&
        value[key] >= 0
      )
        result[key] = value[key];
      else if (text(value[key])) result[key] = value[key];
    }
    for (const key of ["data", "user"]) {
      const child = sanitize(value[key], depth + 1);
      if (child && Object.keys(child).length) result[key] = child;
    }
    return result;
  };
  for (const storage of [localStorage, sessionStorage]) {
    for (const key of [
      "auth_token",
      "auth_user",
      "user",
      "auth",
      "auth_data",
      "access_token",
      "token",
    ]) {
      try {
        const raw = storage.getItem(key);
        if (!raw || raw.length > 65_536) continue;
        if (tokens.has(key)) {
          let value = raw;
          try {
            value = JSON.parse(raw);
          } catch {
            /* plain tokens are also supported */
          }
          if (text(value))
            captured.push({
              [key === "auth_token" ? "access_token" : key]: value,
            });
        } else {
          const value = sanitize(JSON.parse(raw));
          if (value && Object.keys(value).length)
            captured.push(key === "auth_user" ? { user: value } : value);
        }
      } catch {
        /* A malformed/unavailable storage item is not a logged-in session. */
      }
    }
  }
  if (JSON.stringify(captured).length > 196_608)
    throw new Error("登录信息过大，未读取。");
  return { origin: location.origin, captured };
}
