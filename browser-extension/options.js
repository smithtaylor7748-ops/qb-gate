// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 设置页：受保护网站名单、和 QB Gate 的连接、关于。
import { DEFAULT_SITES, normalizeSite, sitePatterns } from "./core.js";
import { $, feedback, renderStatus, runAction, send } from "./ui.js";

let busy = false;
let state = null;
let drawn = "";

function drawSites() {
  $("sites").replaceChildren(
    ...state.sites.map((site) => {
      const item = document.createElement("li");
      item.append(site);
      const remove = document.createElement("button");
      remove.type = "button";
      remove.textContent = "×";
      remove.title = `移除 ${site}`;
      remove.setAttribute("aria-label", `移除 ${site}`);
      remove.disabled = busy || state.sites.length <= 1;
      remove.onclick = () => removeSite(site);
      item.append(remove);
      return item;
    }),
  );
}

async function refresh() {
  state = await send("state");
  renderStatus($("status"), state, onAction, { busy });
  const key = JSON.stringify(state.sites) + busy;
  if (key !== drawn) {
    drawn = key;
    drawSites();
  }
  $("pairing").textContent = state.paired
    ? `已配对（编号 ${state.pairingId}，本机端口 ${state.port}）。${
        state.link === "connected"
          ? "现在连着。"
          : state.linkReason || "现在没连上。"
      }`
    : `还没有配对。${state.linkReason ? `${state.linkReason}。` : ""}`;
  $("forget").disabled = busy || !state.paired;
  $("about").textContent =
    `版本 ${state.version} · 扩展 ID ${state.extensionId}（公开的，用来认插件，不是密码）`;
}

async function run(work) {
  if (busy) return;
  busy = true;
  feedback($("feedback"), "");
  try {
    await work();
  } catch (error) {
    feedback(
      $("feedback"),
      error instanceof Error ? error.message : "操作失败。",
      true,
    );
  } finally {
    busy = false;
    await refresh().catch(() => {});
  }
}

function onAction(action) {
  if (action === "guide") {
    void send("guide");
    return;
  }
  void run(() => runAction(action));
}

function removeSite(site) {
  if (
    !confirm(
      `移除之后，${site} 不再受保护（保护开着时也能直接打开）。确定移除？`,
    )
  )
    return;
  void run(async () => {
    await send("sites", { sites: state.sites.filter((item) => item !== site) });
    if (!DEFAULT_SITES.includes(site))
      await chrome.permissions
        .remove({ origins: sitePatterns([site]) })
        .catch(() => {});
    feedback($("feedback"), `已移除 ${site}。`);
  });
}

$("add").onclick = () => {
  const site = normalizeSite($("site").value);
  if (!site) {
    feedback(
      $("feedback"),
      "填一个网站域名，例如 example.com（不能是 IP 或本机地址）。",
      true,
    );
    return;
  }
  if (state.sites.includes(site)) {
    feedback($("feedback"), `${site} 已经在名单里了。`);
    return;
  }
  // 在点击里直接申请：浏览器只允许用户手势触发可选主机权限。
  const permission = DEFAULT_SITES.includes(site)
    ? Promise.resolve(true)
    : chrome.permissions
        .request({ origins: sitePatterns([site]) })
        .catch(() => false);
  void run(async () => {
    const granted = await permission;
    await send("sites", { sites: [...state.sites, site] });
    $("site").value = "";
    feedback(
      $("feedback"),
      granted
        ? `已添加 ${site}。`
        : `已添加 ${site}，照样会拦；没有允许访问它，所以触发时已经打开的页面不会自动换成说明页。`,
    );
  });
};
$("site").addEventListener("keydown", (event) => {
  if (event.key === "Enter") $("add").click();
});
$("reset").onclick = () => {
  if (
    !confirm(`恢复成默认的 ${DEFAULT_SITES.join("、")}？自己加的网站会被移除。`)
  )
    return;
  void run(async () => {
    const extra = state.sites.filter((site) => !DEFAULT_SITES.includes(site));
    await send("sites", { sites: [...DEFAULT_SITES] });
    if (extra.length)
      await chrome.permissions
        .remove({ origins: sitePatterns(extra) })
        .catch(() => {});
    feedback($("feedback"), "已恢复默认名单。");
  });
};
$("forget").onclick = () => {
  if (!confirm("忘掉和 QB Gate 的连接？之后要在面板重新点「连接浏览器」。"))
    return;
  void run(async () => {
    await send("forget");
    feedback($("feedback"), "已忘掉这个连接。");
  });
};

void refresh().catch((error) => feedback($("feedback"), error.message, true));
setInterval(() => {
  if (!busy) void refresh().catch(() => {});
}, 1_500);
