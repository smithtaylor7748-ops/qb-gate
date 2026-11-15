// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 认领页面自己的脚本：只问桥「这次认领怎么样了」，把结果写成一句话。
// 认领本身由插件的内容脚本做（读地址栏 # 后面的认领码、带着扩展来源回调桥），不在这里。
//
// 给插件的约定（见 docs/DESIGN-NOTES.zh-CN.md「浏览器助手连接协议」）：
//   - <html data-qb-gate-link="2"> 表示这是认领页面；认领码在 location.hash 里，形如 #c=<64 位十六进制>。
//   - 插件开始处理时在 <html> 上写 data-qb-extension="<插件版本>"；
//     认领失败时写 data-qb-extension-error="<一句话原因>"。本页只读这两个属性，不执行里面的内容。
(() => {
  const root = document.documentElement;
  const show = (view) => {
    for (const section of document.querySelectorAll("[data-view]"))
      section.hidden = section.dataset.view !== view;
  };
  const text = (id, value) => {
    const node = document.getElementById(id);
    if (!node) return;
    node.textContent = value;
    node.hidden = !value;
  };

  const match = /^#c=([a-f0-9]{64})$/.exec(location.hash);
  if (!match) {
    show("no-claim");
    return;
  }
  const claim = match[1];
  const started = Date.now();
  let failures = 0;

  async function ask() {
    const response = await fetch("/link/state", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ claim }),
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
    });
    return response.json();
  }

  async function poll() {
    let value;
    try {
      value = await ask();
      failures = 0;
    } catch {
      failures += 1;
      value = { state: failures >= 3 ? "offline" : "pending" };
    }
    switch (value.state) {
      case "claimed":
        text(
          "done-browser",
          typeof value.browser === "string" && value.browser
            ? `QB Gate 里它显示为「${value.browser.slice(0, 80)}」。`
            : "",
        );
        show("done");
        // 认领码已经用掉了，别把它留在地址栏和历史记录里。
        history.replaceState(null, "", "/link");
        return;
      case "expired":
      case "invalid":
        show("expired");
        return;
      case "offline":
        show("offline");
        return;
      default:
        break;
    }
    if (typeof value.problem === "string" && value.problem) {
      text("problem-text", value.problem.slice(0, 300));
      show("problem");
    } else if (root.dataset.qbExtension) {
      text(
        "extension-error",
        (root.dataset.qbExtensionError || "").slice(0, 300),
      );
      show("claiming");
    } else if (Date.now() - started > 4000) {
      show("missing");
    } else {
      show("waiting");
    }
    setTimeout(poll, 1000);
  }

  void poll();
})();
