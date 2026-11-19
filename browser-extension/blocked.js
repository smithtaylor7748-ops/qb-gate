// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 拦截页：说清楚为什么拦、下一步点哪里；放行了（或保护被暂停了）就回到原来的页面。
// 这一页只由插件自己打开（后台切标签页过来），不对网页开放。
import { protectedSite, returnTarget } from "./core.js";
import { $, feedback, renderStatus, runAction, send } from "./ui.js";

let busy = false;
let returning = false;

function goBack(target) {
  if (returning || !target) return;
  returning = true;
  location.replace(target);
}

async function refresh() {
  const state = await send("state");
  const target = returnTarget(location.search);
  const host = target ? new URL(target).hostname : null;
  $("headline").textContent = host ? `${host} 被拦下了` : "受保护网站被拦下了";
  $("target").textContent = target ? `原来要打开：${target}` : "";
  $("return").hidden = !target;
  $("return-note").hidden = !target;
  renderStatus($("status"), state, onAction, { busy });
  // 放行了、暂停了、或者这个网站已经不在名单里了：回原来的页面。
  if (
    state.allowed ||
    !state.protect ||
    (target && !protectedSite(target, state.sites))
  )
    goBack(target);
}

function onAction(action) {
  if (busy) return;
  busy = true;
  feedback($("feedback"), "");
  void runAction(action)
    .catch((error) => feedback($("feedback"), error.message, true))
    .finally(() => {
      busy = false;
      void refresh().catch(() => {});
    });
}

$("return").onclick = () => {
  const target = returnTarget(location.search);
  if (target) location.assign(target);
};
$("options").onclick = () => void send("options");
void refresh().catch((error) => feedback($("feedback"), error.message, true));
setInterval(() => {
  if (!busy) void refresh().catch(() => {});
}, 1_000);
