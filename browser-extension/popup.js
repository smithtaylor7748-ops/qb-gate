// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 弹窗：首屏一个状态 + 一个主按钮；附带功能收在「更多功能」里。
import { webOrigin } from "./core.js";
import { $, feedback, renderStatus, runAction, send, siteList } from "./ui.js";

let state = null;
let stations = [];
let currentOrigin = null;
let busy = false;

function updateTools() {
  const connected = state?.link === "connected";
  for (const id of ["probe", "refresh"]) $(id).disabled = busy || !connected;
  for (const id of ["import", "pricing"])
    $(id).disabled = busy || !connected || !selected();
}

async function refresh() {
  state = await send("state");
  renderStatus($("status"), state, onAction, { busy });
  siteList($("sites"), state.sites);
  $("version").textContent = `版本 ${state.version}`;
  updateTools();
}

async function run(work) {
  if (busy) return;
  busy = true;
  feedback($("feedback"), "");
  if (state) renderStatus($("status"), state, onAction, { busy });
  updateTools();
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

const DONE = {
  enable: "已开启保护。正在核实出口 IP，核实之前受保护网站先拦着。",
  pause: "已暂停保护，拦截规则已全部撤掉。",
  reallow: "已按现在的出口重新核实。",
  retry: "正在重新检查。",
};

function onAction(action) {
  void run(async () => {
    await runAction(action);
    if (action === "guide") window.close();
    else feedback($("feedback"), DONE[action] ?? "");
  });
}

function selected() {
  return stations.find(
    (station) =>
      station.id === $("station").value && station.origin === currentOrigin,
  );
}

async function loadStations() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  currentOrigin = webOrigin(tab?.url);
  $("current-origin").textContent = currentOrigin
    ? `当前页面：${currentOrigin}`
    : "请先打开要读取的中转站网页。";
  const previous = $("station").value;
  stations = await send("stations");
  if (!Array.isArray(stations)) throw new Error("站点列表无法识别。");
  $("station").replaceChildren();
  const matches = stations.filter(
    (station) => station.origin === currentOrigin,
  );
  if (!matches.length) {
    $("station").add(
      new Option("当前地址没有对应的站点，请先在 QB Gate 添加", ""),
    );
  } else {
    for (const station of matches)
      $("station").add(new Option(station.name, station.id));
    if (matches.some((station) => station.id === previous))
      $("station").value = previous;
  }
  updateTools();
}

function stationAction(type) {
  return () => {
    const station = selected();
    if (!station || busy) return;
    // 在点击里直接申请：Chrome 只允许用户手势触发可选主机权限。
    const permission = chrome.permissions.request({
      origins: [`${station.origin}/*`],
    });
    void run(async () => {
      if (!(await permission))
        throw new Error("没有允许访问这个站点，未读取账户或价格。");
      const result = await send(type, {
        stationId: station.id,
        origin: station.origin,
        backend: $("backend").value,
      });
      if (type === "import")
        feedback(
          $("feedback"),
          `已连接${result.account ? `：${result.account}` : "当前账户"}，请回 QB Gate「查套路」使用。`,
        );
      else {
        renderPrices(result);
        feedback($("feedback"), "已读取公开价格，价格对照也已交回 QB Gate。");
      }
    });
  };
}

function renderPrices(result) {
  const root = $("prices");
  root.replaceChildren();
  const notice = document.createElement("p");
  notice.className = "note";
  notice.textContent =
    result.notice ||
    "单位：站点美元额度 / 百万 Token；倍数相对 QB Gate 官方参考价。";
  root.append(notice);
  const money = (value) =>
    typeof value === "number" && Number.isFinite(value)
      ? `$${value.toLocaleString(undefined, { maximumFractionDigits: 8 })}`
      : "未知";
  for (const model of result.models ?? []) {
    const item = document.createElement("div");
    item.className = "price";
    const title = document.createElement("strong");
    title.textContent = `${model.model} · ${model.group_name || model.group || "未指定分组"}`;
    item.append(title);
    const dl = document.createElement("dl");
    ["输入", "缓存读", "缓存写", "输出"].forEach((label, index) => {
      const dt = document.createElement("dt");
      dt.textContent = label;
      const dd = document.createElement("dd");
      const ratio = model.official_multipliers?.[index];
      dd.textContent = `${money(model.prices_per_mtok?.[index])}${
        typeof ratio === "number" && Number.isFinite(ratio)
          ? ` · ${ratio.toFixed(3)}×`
          : ""
      }`;
      dl.append(dt, dd);
    });
    item.append(dl);
    if (
      model.per_request_price !== null &&
      model.per_request_price !== undefined
    ) {
      const price = document.createElement("p");
      price.textContent = `按次收费：${money(model.per_request_price)} / 次`;
      item.append(price);
    }
    if (model.notice) {
      const note = document.createElement("p");
      note.className = "note";
      note.textContent = model.notice;
      item.append(note);
    }
    root.append(item);
  }
}

$("import").onclick = stationAction("import");
$("pricing").onclick = stationAction("pricing");
$("station").onchange = updateTools;
$("refresh").onclick = () => void run(loadStations);
$("probe").onclick = () =>
  void run(async () => {
    await send("probe");
    feedback($("feedback"), "已打开检测页，完成后回 QB Gate 查看结果。");
  });
$("manage").onclick = () => void send("options").then(() => window.close());
$("options").onclick = () => void send("options").then(() => window.close());
$("guide").onclick = () => void send("guide").then(() => window.close());
$("more").addEventListener("toggle", () => {
  if ($("more").open && state?.link === "connected") void run(loadStations);
});

void refresh().catch((error) => feedback($("feedback"), error.message, true));
setInterval(() => {
  if (!busy) void refresh().catch(() => {});
}, 1_000);
