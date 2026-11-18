// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// AGPL-3.0-only; see LICENSE and LICENSE-ADDITIONAL-TERMS.md.
//
// 内容脚本：只在 QB Gate 面板打开的认领页面（http://127.0.0.1:<端口>/link#c=<认领码>）上做事。
// 告诉页面「插件看到了」，把认领码交给后台，后台带着扩展来源去找桥认领（协议第四、五节）。
// 页面只读 <html> 上的两个属性，不执行里面的内容；这里也不读页面别的东西。
(() => {
  const root = document.documentElement;
  if (root.dataset.qbGateLink !== "2") return;
  const match = /^#c=([a-f0-9]{64})$/.exec(location.hash);
  if (!match) return;
  root.dataset.qbExtension = chrome.runtime.getManifest().version;
  delete root.dataset.qbExtensionError;
  const fail = (text) => {
    root.dataset.qbExtensionError = String(text || "插件认领失败").slice(
      0,
      300,
    );
  };
  chrome.runtime.sendMessage({ type: "claim", claim: match[1] }).then(
    (reply) => {
      if (!reply?.ok) fail(reply?.error);
    },
    () => fail("插件后台没有响应，请按 F5 刷新这个页面再试"),
  );
})();
