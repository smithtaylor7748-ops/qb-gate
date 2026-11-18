// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 插件各页面共用：跟后台说话、把「一个状态 + 一个主按钮」画出来。
import { describe } from "./core.js";

export const $ = (id) => document.getElementById(id);

export async function send(type, data = {}) {
  let response;
  try {
    response = await chrome.runtime.sendMessage({ type, ...data });
  } catch {
    throw new Error("插件后台没有响应，请重新打开这个页面。");
  }
  if (!response?.ok)
    throw new Error(
      response?.error || "插件后台没有响应，请重新打开这个页面。",
    );
  return response.value;
}

export function feedback(node, text, error = false) {
  if (!node) return;
  node.textContent = text;
  node.classList.toggle("error", error);
}

/**
 * 在 root 里画状态卡：root 里要有 [data-title]、[data-detail]、[data-actions]。
 * onAction(action) 处理按钮；返回这次的 describe 结果。
 */
export function renderStatus(root, state, onAction, { busy = false } = {}) {
  const view = describe(state);
  root.className = `card status tone-${view.tone}`;
  root.querySelector("[data-title]").textContent = view.title;
  root.querySelector("[data-detail]").textContent = view.detail;
  const actions = root.querySelector("[data-actions]");
  const buttons = [];
  if (view.primary) buttons.push({ ...view.primary, primary: true });
  for (const item of view.secondary) buttons.push(item);
  const signature = JSON.stringify(buttons) + busy;
  if (actions.dataset.signature !== signature) {
    actions.dataset.signature = signature;
    actions.replaceChildren(
      ...buttons.map((item) => {
        const button = document.createElement("button");
        button.type = "button";
        button.textContent = item.label;
        button.className = item.primary ? "primary" : "link";
        button.dataset.action = item.action;
        button.disabled = busy;
        button.addEventListener("click", () => onAction(item.action));
        return button;
      }),
    );
  }
  return view;
}

/** 状态卡上的按钮统一怎么做（弹窗里「连接 QB Gate」打开引导页后顺手关掉弹窗）。 */
export async function runAction(action) {
  switch (action) {
    case "guide":
      return send("guide");
    case "enable":
    case "pause":
    case "reallow":
    case "retry":
      return send(action);
    default:
      throw new Error("未知操作。");
  }
}

export function siteList(node, sites) {
  node.replaceChildren(
    ...sites.map((site) => {
      const item = document.createElement("li");
      item.className = "plain";
      item.textContent = site;
      return item;
    }),
  );
}
