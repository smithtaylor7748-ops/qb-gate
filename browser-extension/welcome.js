// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 引导页：装好之后自动打开。三步的勾随实际状态变，第三步就地开启保护。
import { $, feedback, renderStatus, runAction, send, siteList } from "./ui.js";

let busy = false;
let state = null;

async function refresh() {
  state = await send("state");
  $("step-connect").classList.toggle("done", state.paired);
  $("step-confirm").classList.toggle(
    "done",
    state.paired && state.link === "connected",
  );
  $("step-protect").classList.toggle("done", state.protect);
  $("enable").disabled = busy || state.protect || state.link !== "connected";
  $("enable").textContent = state.protect ? "保护已开启" : "开启保护";
  siteList($("sites"), state.sites);
  renderStatus($("status"), state, onAction, { busy });
}

function onAction(action) {
  if (action === "guide") {
    $("step-connect").scrollIntoView({ behavior: "smooth" });
    return;
  }
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

$("enable").onclick = () => onAction("enable");
$("options").onclick = () => void send("options");
void refresh().catch((error) => feedback($("feedback"), error.message, true));
setInterval(() => {
  if (!busy) void refresh().catch(() => {});
}, 1_000);
