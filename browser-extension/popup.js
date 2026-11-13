// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
import { webOrigin } from "./core.js";

const $ = (id) => document.getElementById(id);
let state = null;
let stations = [];
let currentOrigin = null;
let busy = false;

async function send(type, data = {}) {
  const response = await chrome.runtime.sendMessage({ type, ...data });
  if (!response?.ok)
    throw new Error(response?.error || "浏览器后台没有响应，请重新打开插件。");
  return response.value;
}
function feedback(text, error = false) {
  $("feedback").textContent = text;
  $("feedback").classList.toggle("error", error);
}
function updateButtons() {
  for (const id of [
    "arm",
    "disarm",
    "probe",
    "refresh",
    "import",
    "pricing",
    "pair",
    "reconnect",
    "forget",
  ])
    $(id).disabled = busy;
  if (busy) return;
  $("arm").disabled =
    !state?.connected || state.armed || state.armPending || state.closing;
  $("disarm").disabled = !state?.armed && !state?.armPending;
  for (const id of ["probe", "refresh"]) $(id).disabled = !state?.connected;
  for (const id of ["import", "pricing"])
    $(id).disabled = !state?.connected || !selected();
  $("reconnect").disabled = !state?.paired || state.connected;
  $("forget").disabled = !state?.paired;
}
async function refreshState() {
  state = await send("state");
  $("connection").textContent = state.connected ? "已连接" : "未连接";
  $("connection").classList.toggle("good", state.connected);
  $("protection").textContent = state.armed
    ? "保护中"
    : state.armPending
      ? "核验中"
      : "未开启";
  $("protection").classList.toggle("good", state.armed);
  $("reason").textContent =
    `${state.reason}${state.ip ? ` · ${state.ip}` : ""}`;
  if (!state.paired) $("pairing-details").open = true;
  updateButtons();
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
    ? `当前站点：${currentOrigin}`
    : "请打开要读取的中转站网页。";
  const previous = $("station").value;
  stations = await send("stations");
  if (!Array.isArray(stations)) throw new Error("站点列表无法识别。");
  $("station").replaceChildren();
  const matches = stations.filter(
    (station) => station.origin === currentOrigin,
  );
  if (!matches.length) {
    $("station").add(new Option("当前地址未匹配站点，请先在 QB Gate 添加", ""));
  } else {
    for (const station of matches)
      $("station").add(new Option(station.name, station.id));
    if (matches.some((station) => station.id === previous))
      $("station").value = previous;
  }
  updateButtons();
}
async function run(work) {
  if (busy) return;
  busy = true;
  updateButtons();
  feedback("");
  try {
    await work();
  } catch (error) {
    feedback(error instanceof Error ? error.message : "操作失败。", true);
  } finally {
    busy = false;
    await refreshState().catch(() => updateButtons());
  }
}
function command(type, success) {
  return () =>
    void run(async () => {
      await send(type);
      feedback(success);
    });
}
$("arm").onclick = command("arm", "保护已开启。关闭插件弹窗不会停止保护。");
$("disarm").onclick = command("disarm", "保护已停止。");
$("probe").onclick = command(
  "probe",
  "已打开检测页，完成后请回 QB Gate 查看结果。",
);
$("forget").onclick = command("forget", "已停止保护并移除配对信息。");
$("reconnect").onclick = () =>
  void run(async () => {
    await send("connect");
    await loadStations();
    feedback("已重新连接。");
  });
$("pair").onclick = () =>
  void run(async () => {
    await send("pair", { pairing: $("pairing").value });
    $("pairing").value = "";
    $("pairing-details").open = false;
    await loadStations();
    feedback("配对成功。需要保护浏览器时，请点击「开启保护」。");
  });
$("refresh").onclick = () => void run(loadStations);
$("station").onchange = updateButtons;

function stationAction(type) {
  return () => {
    const station = selected();
    if (!station || busy) return;
    // Request from the click itself: Chrome requires a user gesture for optional host grants.
    const permission = chrome.permissions.request({
      origins: [`${station.origin}/*`],
    });
    void run(async () => {
      if (!(await permission))
        throw new Error("未授权访问此站点，未读取账户或价格。");
      const result = await send(type, {
        stationId: station.id,
        origin: station.origin,
        backend: $("backend").value,
      });
      if (type === "import")
        feedback(
          `已连接${result.account ? `：${result.account}` : "当前账户"}，请回 QB Gate「查套路」使用。`,
        );
      else {
        renderPrices(result);
        feedback("已读取公开价格，价格对照也已交回 QB Gate。");
      }
    });
  };
}
$("import").onclick = stationAction("import");
$("pricing").onclick = stationAction("pricing");

function renderPrices(result) {
  const root = $("prices");
  root.replaceChildren();
  const notice = document.createElement("p");
  notice.className = "note";
  notice.textContent =
    result.notice ||
    "单位：站点美元额度 / 百万 Token；倍数相对 QB Gate 官方参考价。";
  root.append(notice);
  const list = document.createElement("div");
  list.className = "prices-list";
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
      dd.textContent = `${money(model.prices_per_mtok?.[index])}${typeof ratio === "number" && Number.isFinite(ratio) ? ` · ${ratio.toFixed(3)}×` : ""}`;
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
    list.append(item);
  }
  root.append(list);
}

void refreshState()
  .then(() => state.connected && loadStations())
  .catch(() => feedback("暂时无法连接浏览器后台，请重新打开插件。", true));
setInterval(() => {
  if (!busy) void refreshState().catch(() => {});
}, 2_000);
